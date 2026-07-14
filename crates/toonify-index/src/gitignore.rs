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
}
