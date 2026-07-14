//! Shared stdin/stdout/file I/O helpers for the `convert`/`check`/`pipe`
//! subcommands. `-` conventionally means stdin/stdout throughout the CLI
//! contract (`contracts/cli.md`).

use std::io::{self, Read as _, Write as _};
use std::path::Path;

/// Reads all of `path`'s bytes, or all of stdin when `path == "-"`. A
/// read-only operation in both cases -- never opens `path` for writing, so
/// it can never mutate a source file (FR-005).
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
