// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned wrap -- <command...>`
//!
//! Runs `<command...>`, captures stdout, feeds it through the adaptive
//! in-place conversion path, prints the result; stderr and exit code of the
//! wrapped command are passed through unchanged.

use std::io::{self, Read as _, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use clap::Args;

use crate::cli::FormatHint;

#[derive(Debug, Args)]
#[allow(clippy::struct_excessive_bools)]
pub struct WrapArgs {
    /// Force the parser's doc type instead of relying on content-sniffing.
    #[arg(short = 'f', long = "format-hint", value_enum)]
    pub format_hint: Option<FormatHint>,

    /// Minimum savings margin, as a percentage, required to convert (default 2%).
    #[arg(short = 'm', long)]
    pub margin: Option<f64>,

    /// Maximum input size in bytes before hard passthrough (default 2 MiB).
    #[arg(short = 'b', long = "max-bytes")]
    pub max_bytes: Option<u64>,

    /// Enable the dictionary-compression tier (#1). Default: on.
    #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with = "no_dict")]
    pub dict: bool,
    /// Disable the dictionary-compression tier (#1).
    #[arg(long = "no-dict", action = clap::ArgAction::SetTrue, conflicts_with = "dict")]
    pub no_dict: bool,

    /// Enable the density-aware acceptance margin (#2). Default: on.
    #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with = "no_auto_margin")]
    pub auto_margin: bool,
    /// Disable the density-aware acceptance margin (#2).
    #[arg(long = "no-auto-margin", action = clap::ArgAction::SetTrue, conflicts_with = "auto_margin")]
    pub no_auto_margin: bool,

    /// Enable the entropy gate (#5). Default: on.
    #[arg(long, action = clap::ArgAction::SetTrue, conflicts_with = "no_entropy_gate")]
    pub entropy_gate: bool,
    /// Disable the entropy gate (#5).
    #[arg(long = "no-entropy-gate", action = clap::ArgAction::SetTrue, conflicts_with = "entropy_gate")]
    pub no_entropy_gate: bool,

    /// Protect these column/key substrings from dictionary abbreviation (#3).
    /// Repeatable.
    #[arg(long = "protect", action = clap::ArgAction::Append)]
    pub protect: Vec<String>,

    /// Path to a tooned config file.
    #[arg(short = 'c', long)]
    pub config: Option<PathBuf>,

    /// Write output to this file instead of stdout.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub out: Option<PathBuf>,

    /// The command (and its arguments) to run, after `--`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub command: Vec<String>,
}

/// Collapse the `--flag` / `--no-flag` pair into a single `Option<bool>`.
fn flag_value(yes: bool, no: bool) -> Option<bool> {
    if no {
        Some(false)
    } else if yes {
        Some(true)
    } else {
        None
    }
}

/// `code()` is `None` only when the process was killed by a signal (no
/// POSIX exit code to mirror); 1 is a reasonable generic-failure fallback
/// for that case, not a silent-default masking of a real value.
const SIGNAL_KILLED_FALLBACK_CODE: i32 = 1;

/// Size of the streaming copy buffer used once the wrapped command's stdout
/// is known to exceed `max_input_bytes` and further bytes are just piped
/// straight through instead of being accumulated in memory.
const STREAM_CHUNK_BYTES: usize = 64 * 1024;

/// RAII guard that removes a temporary file unless explicitly disarmed.
struct TempGuard {
    path: PathBuf,
    armed: bool,
}

impl TempGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

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

    let config = crate::config::Config::load(args.config.as_deref())?;
    let mut opts = config.conversion_options(
        args.margin,
        args.max_bytes,
        args.format_hint,
        None,
        flag_value(args.dict, args.no_dict),
        flag_value(args.auto_margin, args.no_auto_margin),
        flag_value(args.entropy_gate, args.no_entropy_gate),
        if args.protect.is_empty() { None } else { Some(args.protect.clone()) },
        None,
        None,
    );
    // `tooned wrap` is a streaming hot path; prefer the zero-allocation
    // fast path by default. `TOONED_WRAP_ZERO_ALLOC=0` falls back to the
    // full `maybe_tooned` pipeline (dictionary/entropy/critical-field tiers).
    opts.zero_alloc = match std::env::var("TOONED_WRAP_ZERO_ALLOC") {
        Ok(v) => v != "0",
        Err(_) => true,
    };

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
    let is_stdout = args.out.as_deref().is_none_or(|p| p == Path::new("-"));
    // Cap the initial allocation and avoid `cap as u64 + 1` overflow when
    // `cap` is near `usize::MAX` (mirrors the `read_bounded` hardening in
    // `io.rs`).
    let mut buf = Vec::with_capacity(cap.min(64 * 1024).saturating_add(1));
    let read_result = (&mut stdout_pipe).take((cap as u64).saturating_add(1)).read_to_end(&mut buf);

    if let Err(err) = read_result {
        // A genuine I/O error reading the pipe -- fall back to passing
        // whatever was captured through unchanged rather than losing it.
        // For stdout (including `-`) a broken pipe is not this wrapper's
        // error to escalate; file destinations still report failures.
        if let Err(write_err) = crate::cli::io::write_output(args.out.as_deref(), &buf)
            && !is_stdout
        {
            eprintln!("tooned wrap: failed to write captured output: {write_err}");
            drop(stdout_pipe);
            let _ = child.wait();
            std::process::exit(1);
        }
        drop(stdout_pipe);
        let status = child.wait();
        eprintln!("tooned wrap: error reading child stdout: {err}");
        let code = match status.ok().and_then(|s| s.code()) {
            Some(code) => code,
            None => SIGNAL_KILLED_FALLBACK_CODE,
        };
        std::process::exit(code);
    }

    if buf.len() as u64 <= cap as u64 {
        // Entire output fits within the cap: run it through the normal
        // adaptive conversion path, then write it atomically when a file is
        // requested so the destination is never observed partially written.
        let mut toon_out = String::new();
        let (output, input_len, output_len, converted) =
            crate::cli::io::maybe_tooned_output(&buf, &opts, &mut toon_out);
        crate::metrics_recorder::record_convert_outcome(
            crate::metrics_recorder::CliSurface::Wrap,
            &crate::metrics_recorder::SourceLabel::None,
            None,
            converted,
            input_len,
            output_len,
        );
        if let Err(err) = crate::cli::io::write_output(args.out.as_deref(), output.as_ref())
            && !is_stdout
        {
            return Err(anyhow::anyhow!("tooned wrap: failed to write output: {err}"));
        }
    } else {
        // Output exceeds the cap: it would be a guaranteed passthrough, so
        // write the buffered prefix and stream the rest straight through
        // without ever holding it all in memory at once.
        //
        // For file destinations, stream into a same-directory temp file and
        // promote it with an atomic rename only after the child stdout is fully
        // drained. This avoids opening/truncating `--out` before the wrapped
        // command finishes reading it (e.g. `wrap --out data.json -- cat data.json`).
        let mut writer: io::BufWriter<Box<dyn Write>>;
        let mut output_state: Option<(PathBuf, PathBuf, TempGuard)> = None;
        if is_stdout {
            writer = crate::cli::io::output_writer(args.out.as_deref())?;
        } else {
            let Some(path) = args.out.as_deref() else {
                anyhow::bail!("tooned wrap: --out path required for non-stdout destination");
            };
            let target = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            let (tmp_path, file) = crate::cli::io::open_output_temp(&target)?;
            let guard = TempGuard::new(tmp_path.clone());
            writer = io::BufWriter::new(file);
            output_state = Some((target, tmp_path, guard));
        }

        let mut write_failed = false;
        if let Err(err) = writer.write_all(&buf) {
            if !is_stdout {
                return Err(anyhow::anyhow!("tooned wrap: failed to write output: {err}"));
            }
            write_failed = true;
        }
        drop(buf);

        if !write_failed {
            let mut chunk = vec![0u8; STREAM_CHUNK_BYTES];
            loop {
                let n = match stdout_pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                if let Some(written) = chunk.get(..n)
                    && let Err(err) = writer.write_all(written)
                {
                    if !is_stdout {
                        return Err(anyhow::anyhow!("tooned wrap: failed to write output: {err}"));
                    }
                    write_failed = true;
                    break;
                }
            }
        }

        if !write_failed
            && let Err(err) = writer.flush()
            && !is_stdout
        {
            return Err(anyhow::anyhow!("tooned wrap: failed to flush output: {err}"));
        }

        if let Some((target, tmp_path, mut guard)) = output_state {
            crate::cli::io::atomic_rename(&tmp_path, &target)
                .map_err(|err| anyhow::anyhow!("tooned wrap: failed to write output: {err}"))?;
            guard.disarm();
        }
    }

    // Close the read end of the pipe now that we are done consuming stdout.
    // For stdout-mode broken-pipe failures this lets the child receive SIGPIPE
    // and terminate instead of blocking on a full pipe buffer.
    drop(stdout_pipe);

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
