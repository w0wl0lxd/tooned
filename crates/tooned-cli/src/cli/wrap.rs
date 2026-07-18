// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned wrap -- <command...>`
//!
//! Runs `<command...>`, captures stdout, feeds it through the same adaptive
//! path, prints the result; stderr and exit code of the wrapped command are
//! passed through unchanged.

use std::io::{Read as _, Write as _};
use std::process::{Command, Stdio};

use clap::Args;
use tooned_core::{Conversion, ConversionOptions};

#[derive(Debug, Args)]
pub struct WrapArgs {
    /// The command (and its arguments) to run, after `--`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

/// `code()` is `None` only when the process was killed by a signal (no
/// POSIX exit code to mirror); 1 is a reasonable generic-failure fallback
/// for that case, not a silent-default masking of a real value.
const SIGNAL_KILLED_FALLBACK_CODE: i32 = 1;

/// Size of the streaming copy buffer used once the wrapped command's stdout
/// is known to exceed `max_input_bytes` and further bytes are just piped
/// straight through instead of being accumulated in memory.
const STREAM_CHUNK_BYTES: usize = 64 * 1024;

pub fn run(args: &WrapArgs) -> anyhow::Result<()> {
    let Some((program, rest)) = args.command.split_first() else {
        anyhow::bail!("tooned wrap: no command given after `--`");
    };

    // stderr is inherited untouched (passed through unchanged per the
    // contract); only stdout is captured so it can be run through the
    // adaptive conversion path.
    let mut child = match Command::new(program)
        .args(rest)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            // A genuine I/O-level problem (e.g. command not found) -- not
            // payload-driven ambiguity, so this is a real CLI error.
            anyhow::bail!("tooned wrap: failed to spawn `{program}`: {err}");
        }
    };

    let opts = ConversionOptions::default();

    // Bound how much of the wrapped command's stdout is ever buffered in
    // memory: read up to `max_input_bytes + 1` bytes (the `+1` is only to
    // detect whether the true output exceeds the cap). If the output turns
    // out to be larger than the cap, `maybe_tooned` would passthrough it
    // unchanged anyway (`PassthroughReason::InputTooLarge`) -- so once that's
    // known, the already-buffered prefix is flushed and the remainder is
    // streamed straight through in small fixed-size chunks instead of being
    // accumulated, keeping peak memory bounded regardless of how large the
    // wrapped command's real output is.
    let Some(mut stdout_pipe) = child.stdout.take() else {
        // Never expected (stdout was just requested via `Stdio::piped()`),
        // but per fail-safe Principle I this must not panic even if it
        // somehow happened -- fall back to mirroring only the exit code.
        let status = child.wait();
        let code = match status.ok().and_then(|s| s.code()) {
            Some(code) => code,
            None => SIGNAL_KILLED_FALLBACK_CODE,
        };
        std::process::exit(code);
    };
    let cap = opts.max_input_bytes;
    // Cap the initial allocation and avoid `cap as u64 + 1` overflow when
    // `cap` is near `usize::MAX` (mirrors the `read_bounded` hardening in
    // `io.rs`).
    let mut buf = Vec::with_capacity(cap.min(64 * 1024).saturating_add(1));
    let read_result = (&mut stdout_pipe).take((cap as u64).saturating_add(1)).read_to_end(&mut buf);

    if let Err(err) = read_result {
        // A genuine I/O error reading the pipe -- fall back to passing
        // whatever was captured through unchanged rather than losing it.
        let _ = std::io::stdout().write_all(&buf);
        let status = child.wait();
        eprintln!("tooned wrap: error reading child stdout: {err}");
        let code = match status.ok().and_then(|s| s.code()) {
            Some(code) => code,
            None => SIGNAL_KILLED_FALLBACK_CODE,
        };
        std::process::exit(code);
    }

    let out_stdout = std::io::stdout();
    if buf.len() as u64 <= cap as u64 {
        // Entire output fits within the cap: run it through the normal
        // adaptive conversion path.
        let converted = match tooned_core::maybe_tooned(&buf, &opts) {
            Ok(Conversion::Toon { text, .. }) => {
                #[allow(clippy::manual_unwrap_or)]
                let buf_len = match buf.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                };
                #[allow(clippy::manual_unwrap_or)]
                let text_len = match text.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                };
                crate::metrics_recorder::record_convert_outcome(
                    crate::metrics_recorder::CliSurface::Wrap,
                    &crate::metrics_recorder::SourceLabel::None,
                    None,
                    true,
                    buf_len,
                    text_len,
                );
                text.into_owned().into_bytes()
            }
            Ok(Conversion::Passthrough { bytes: pt_bytes, .. }) => {
                #[allow(clippy::manual_unwrap_or)]
                let buf_len = match buf.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                };
                #[allow(clippy::manual_unwrap_or)]
                let bytes_len = match pt_bytes.len().try_into() {
                    Ok(v) => v,
                    Err(_) => i64::MAX,
                };
                crate::metrics_recorder::record_convert_outcome(
                    crate::metrics_recorder::CliSurface::Wrap,
                    &crate::metrics_recorder::SourceLabel::None,
                    None,
                    false,
                    buf_len,
                    bytes_len,
                );
                pt_bytes.into_owned()
            }
            Ok(Conversion::Rejected { .. }) | Err(_) => buf,
        };
        let _ = out_stdout.lock().write_all(&converted);
    } else {
        // Output exceeds the cap: it would be a guaranteed passthrough, so
        // write the buffered prefix and stream the rest straight through
        // without ever holding it all in memory at once.
        let mut lock = out_stdout.lock();
        let _ = lock.write_all(&buf);
        drop(buf);
        let mut chunk = vec![0u8; STREAM_CHUNK_BYTES];
        loop {
            let n = match stdout_pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            if let Some(written) = chunk.get(..n) {
                let _ = lock.write_all(written);
            }
        }
    }

    // Mirror the wrapped command's exit code exactly.
    let status = match child.wait() {
        Ok(status) => status,
        Err(err) => {
            anyhow::bail!("tooned wrap: failed to wait on `{program}`: {err}");
        }
    };
    let code = match status.code() {
        Some(code) => code,
        None => SIGNAL_KILLED_FALLBACK_CODE,
    };
    std::process::exit(code);
}
