// SPDX-License-Identifier: AGPL-3.0-only

//! Shared stdin/stdout/file I/O helpers for the `convert`/`check`/`pipe`
//! subcommands. `-` conventionally means stdin/stdout throughout the CLI
//! contract (`contracts/cli.md`).

use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};

/// Reads all of `path`'s bytes, or all of stdin when `path == "-"`. A
/// read-only operation in both cases -- never opens `path` for writing, so
/// it can never mutate a source file (FR-005).
///
/// Unbounded by design -- used by the `--to onto`/`--to tron` encode paths,
/// which enforce their own `max_input_bytes` gate internally (a larger input
/// is a verbatim passthrough, never an error). The `--to json` decode path
/// uses [`read_input_bounded`] instead so it cannot materialize an
/// arbitrarily large file before the decoder's own cap is consulted.
pub fn read_input(path: &Path) -> io::Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        return Ok(buf);
    }
    std::fs::read(path)
}

/// Reads `path`'s bytes (or stdin when `path == "-"`), refusing to materialize
/// more than `cap` bytes in memory. Used by the `--to json` decode path so a
/// large input can never be buffered in full before the decoder's own
/// `max_input_bytes` gate is consulted (the unbounded [`read_input`] is unsafe
/// here because the decode direction has no size gate of its own -- a local
/// denial-of-memory vector). Anything larger than `cap` can never be
/// converted, so there is no reason to buffer the whole file first. Returns an
/// error if the input exceeds `cap`.
pub fn read_input_bounded(path: &Path, cap: usize) -> io::Result<Vec<u8>> {
    // For regular files we can short-circuit on the metadata size, but for
    // stdin, FIFOs, device files, and files that may grow between the stat
    // and the read, we still enforce the cap while reading.
    if path != Path::new("-") {
        let meta = std::fs::metadata(path)?;
        if meta.is_file() && meta.len() > cap as u64 {
            return Err(io::Error::new(
                io::ErrorKind::OutOfMemory,
                "input exceeds the decode size cap",
            ));
        }
    }
    let reader = open_input(path)?;
    let mut buf = Vec::new();
    reader.take((cap as u64).saturating_add(1)).read_to_end(&mut buf)?;
    if buf.len() > cap {
        return Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "input exceeds the decode size cap",
        ));
    }
    Ok(buf)
}

/// Returns a buffered writer for `out`: stdout when `out` is `None` or
/// `Some("-")`, or a newly created/truncated file otherwise.
///
/// This is the non-atomic, streaming counterpart to [`write_output`]: it
/// writes in place rather than through a temp-file-then-rename, so callers
/// that need atomicity for small, known payloads should prefer
/// [`write_output`]. Callers that may stream an unbounded amount of data
/// (e.g. `pipe`/`wrap` passthrough) use this instead.
pub fn output_writer(out: Option<&Path>) -> io::Result<io::BufWriter<Box<dyn io::Write>>> {
    let w: Box<dyn io::Write> = match out {
        Some(path) if path != Path::new("-") => {
            let file =
                std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
            Box::new(file)
        }
        _ => Box::new(io::stdout()),
    };
    Ok(io::BufWriter::new(w))
}

/// Writes `bytes` to `out`, or to stdout when `out` is `None` or `Some("-")`.
/// For a file path the write goes through the same temp-file-then-rename
/// atomic path used by [`write_atomic`] so the destination is never observed
/// partially written.
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
        Some(path) => write_atomic(path, bytes),
    }
}

/// Writes `bytes` to `path` atomically: the data is first written in full
/// to a uniquely-named temp file in the same directory, then promoted into
/// place with a single `rename`. A same-directory rename is atomic on all
/// platforms `tooned` targets, so readers (or a concurrent writer) never
/// observe a partially-written file, unlike a direct in-place `fs::write`.
///
/// If `path` is a symlink, it is resolved to its target (matching the
/// semantics of the direct-write path it replaces) and the symlink itself is
/// left in place.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    // Resolve symlinks so we update the real file, not replace a symlink
    // entry, matching `std::fs::write` semantics. If canonicalization fails
    // (should not happen for an existing input), fall back to the raw path.
    let target = match std::fs::canonicalize(path) {
        Ok(canonical) => canonical,
        Err(_) => path.to_path_buf(),
    };

    let parent =
        target.parent().ok_or_else(|| io::Error::other("target path has no parent directory"))?;
    let file_name = target
        .file_name()
        .ok_or_else(|| io::Error::other("target path has no file name"))?
        .to_string_lossy();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp_name = format!(".{file_name}.tmp.{}.{nanos}", std::process::id());
    let tmp_path = parent.join(tmp_name);

    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp_path)?;
    file.write_all(bytes)?;
    file.flush()?;

    // Best-effort preservation of the original file's mode.
    if let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&tmp_path, meta.permissions());
    }

    match std::fs::rename(&tmp_path, &target) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp_path);
            Err(err)
        }
    }
}

/// Opens a uniquely-named temp file in the same directory as `path` and
/// returns its path together with a writer. The caller must rename the temp
/// path to `path` on success (or delete it on failure).
pub fn open_output_temp(path: &Path) -> io::Result<(PathBuf, Box<dyn io::Write>)> {
    let parent =
        path.parent().ok_or_else(|| io::Error::other("output path has no parent directory"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::other("output path has no file name"))?
        .to_string_lossy();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp_name = format!(".{file_name}.tmp.{}.{nanos}", std::process::id());
    let tmp_path = parent.join(tmp_name);
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp_path)?;
    Ok((tmp_path, Box::new(file)))
}

/// Promotes `tmp_path` to `target` with a same-directory `rename`, preserving
/// the target's existing permissions when possible, and deletes the temp file
/// if the rename fails.
pub fn atomic_rename(tmp_path: &Path, target: &Path) -> io::Result<()> {
    if let Ok(meta) = std::fs::metadata(target) {
        let _ = std::fs::set_permissions(tmp_path, meta.permissions());
    }
    match std::fs::rename(tmp_path, target) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = std::fs::remove_file(tmp_path);
            Err(err)
        }
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
