//! Idempotent `.tooned/` append to the scanned project's `.gitignore`
//! (T058, FR-020, research.md #5): on first index creation, ensure
//! `.tooned/` is covered, creating `.gitignore` if it doesn't exist yet.
//! Never duplicates the entry on a later run.

use std::path::Path;

use crate::IndexError;

const IGNORE_ENTRY: &str = ".tooned/";

/// Idempotently ensures `project_root/.gitignore` covers `.tooned/`.
/// Creates `.gitignore` if it's absent; appends the entry if the file
/// exists but doesn't already cover it (exact-match against `.tooned`,
/// `.tooned/`, `/.tooned`, `/.tooned/` -- the handful of equivalent ways
/// a human might already have written this rule); does nothing (no write
/// at all) if it's already present, so re-running `index` never duplicates
/// the entry.
pub fn ensure_ignored(project_root: &Path) -> Result<(), IndexError> {
    let gitignore_path = project_root.join(".gitignore");

    // Refuse to follow a symlink at the `.gitignore` path: `project_root`
    // can come from an unrestricted, client-supplied path (MCP
    // `tooned_index_build`), so a pre-placed `.gitignore` symlink must not
    // let `std::fs::write` below silently clobber an arbitrary file the
    // process happens to have write access to. `symlink_metadata` (unlike
    // `metadata`) never follows the link itself, so this check is safe even
    // when the link target doesn't exist or isn't readable.
    if let Ok(meta) = std::fs::symlink_metadata(&gitignore_path)
        && meta.file_type().is_symlink()
    {
        return Err(IndexError::Io(std::io::Error::other(format!(
            "refusing to write through a symlinked .gitignore at {}",
            gitignore_path.display()
        ))));
    }

    let existing = match std::fs::read_to_string(&gitignore_path) {
        Ok(contents) => contents,
        // Absent `.gitignore` is the normal, expected case (first index
        // creation) -- start from empty content. Any other read failure
        // (e.g. permission denied) must propagate, not be silently treated
        // as "empty", which would risk overwriting a `.gitignore` we
        // simply failed to read.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(IndexError::Io(err)),
    };

    if already_covers_tooned(&existing) {
        return Ok(());
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(IGNORE_ENTRY);
    updated.push('\n');

    std::fs::write(&gitignore_path, updated)?;
    Ok(())
}

fn already_covers_tooned(contents: &str) -> bool {
    contents
        .lines()
        .any(|line| matches!(line.trim(), ".tooned/" | ".tooned" | "/.tooned/" | "/.tooned"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_existing_entry_variants() {
        assert!(already_covers_tooned(".tooned/\n"));
        assert!(already_covers_tooned(".tooned\n"));
        assert!(already_covers_tooned("/.tooned/\n"));
        assert!(already_covers_tooned("target/\n.tooned/\nnode_modules/\n"));
        assert!(!already_covers_tooned("target/\n"));
        assert!(!already_covers_tooned(""));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_write_through_a_symlinked_gitignore() {
        let dir = tempfile::tempdir().expect("tempdir");
        let real_target = dir.path().join("real-secret-file");
        std::fs::write(&real_target, "do not touch\n").expect("write real target");

        let gitignore_path = dir.path().join(".gitignore");
        std::os::unix::fs::symlink(&real_target, &gitignore_path).expect("create symlink");

        let result = ensure_ignored(dir.path());
        assert!(result.is_err(), "must refuse a symlinked .gitignore, got {result:?}");

        let unchanged = std::fs::read_to_string(&real_target).expect("read real target");
        assert_eq!(unchanged, "do not touch\n", "the symlink target must be left untouched");
    }
}
