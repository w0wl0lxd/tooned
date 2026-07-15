//! Shared stdin/stdout/file I/O helpers for the `convert`/`check`/`pipe`
//! subcommands. `-` conventionally means stdin/stdout throughout the CLI
//! contract (`contracts/cli.md`).

use std::io::{self, Read as _, Write as _};
use std::path::Path;

/// Reads all of `path`'s bytes, or all of stdin when `path == "-"`. A
/// read-only operation in both cases -- never opens `path` for writing, so
/// it can never mutate a source file (FR-005).
///
/// Unbounded by design -- only used by `convert --to json`, whose decode
/// path has no `max_input_bytes` gate of its own (unlike the adaptive
/// paths, which use [`read_bounded`] below instead).
pub fn read_input(path: &Path) -> io::Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        return Ok(buf);
    }
    std::fs::read(path)
}

/// Writes `bytes` to `out`, or to stdout when `out` is `None` or `Some("-")`.
pub fn write_output(out: Option<&Path>, bytes: &[u8]) -> io::Result<()> {
    match out {
        None => {
            io::stdout().write_all(bytes)?;
            Ok(())
        }
        Some(path) if path == Path::new("-") => {
            io::stdout().write_all(bytes)?;
            Ok(())
        }
        Some(path) => std::fs::write(path, bytes),
    }
}

/// Opens `path` for reading, or stdin when `path == "-"`.
pub fn open_input(path: &Path) -> io::Result<Box<dyn io::Read>> {
    if path == Path::new("-") {
        Ok(Box::new(io::stdin()))
    } else {
        Ok(Box::new(std::fs::File::open(path)?))
    }
}

/// Opens `out` for writing, or stdout when `out` is `None` or `Some("-")`.
pub fn open_output(out: Option<&Path>) -> io::Result<Box<dyn io::Write>> {
    match out {
        None => Ok(Box::new(io::stdout())),
        Some(path) if path == Path::new("-") => Ok(Box::new(io::stdout())),
        Some(path) => Ok(Box::new(std::fs::File::create(path)?)),
    }
}

/// Outcome of [`read_bounded`].
pub enum BoundedRead {
    /// The input was `<= cap` bytes and is fully buffered here, same as a
    /// plain `read_to_end`/`fs::read` would have produced.
    Fits(Vec<u8>),
    /// The input was larger than `cap` bytes. Since anything above
    /// `ConversionOptions::max_input_bytes` is unconditionally an
    /// `InputTooLarge` passthrough regardless of content, the original
    /// bytes were already streamed verbatim to the caller-supplied `out`
    /// without ever being buffered in full; `total_bytes` is the exact
    /// input size, for callers that only need it for reporting.
    Streamed { total_bytes: u64 },
}

/// Reads from `reader`, buffering at most `cap + 1` bytes at a time --
/// never the whole input unconditionally the way `read_input` does. Used by
/// the `convert`/`check` adaptive paths, which (unlike `--to json` decode)
/// go through `maybe_tooned`/`inspect`'s `max_input_bytes` gate, so
/// "larger than `cap`" and "guaranteed unchanged passthrough" are
/// equivalent -- meaning a multi-GB input never needs to be materialized in
/// memory at all to know the outcome (finding: unbounded `read_to_end`/
/// `fs::read` previously ran before that size cap was ever consulted).
pub fn read_bounded(
    reader: &mut dyn io::Read,
    cap: usize,
    out: &mut dyn io::Write,
) -> io::Result<BoundedRead> {
    // Cap the initial allocation so `cap` near `usize::MAX` doesn't try to
    // reserve an impossible buffer; `read_to_end` will grow up to `take_limit`
    // bytes as needed.
    let mut buf = Vec::with_capacity(cap.min(64 * 1024).saturating_add(1));
    let take_limit = (cap as u64).saturating_add(1);
    (&mut *reader).take(take_limit).read_to_end(&mut buf)?;
    if buf.len() <= cap {
        return Ok(BoundedRead::Fits(buf));
    }

    let mut total_bytes = buf.len() as u64;
    out.write_all(&buf)?;
    drop(buf);

    // Stream the remainder straight through in small fixed-size chunks
    // rather than buffering it, keeping peak memory bounded regardless of
    // how large the true input is (mirrors `wrap.rs`'s existing
    // streaming-passthrough strategy for the same situation).
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        total_bytes += n as u64;
        if let Some(written) = chunk.get(..n) {
            out.write_all(written)?;
        }
    }
    Ok(BoundedRead::Streamed { total_bytes })
}
