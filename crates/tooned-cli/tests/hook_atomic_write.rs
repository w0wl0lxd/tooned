// SPDX-License-Identifier: AGPL-3.0-only

//! T071: config writes for `hook install`/`hook uninstall` must go through a
//! temp-file-then-rename (atomic) path in the target directory, never a
//! direct in-place write -- guards against concurrent-writer corruption
//! (spec.md Edge Cases: concurrent installer runs). This test asserts the
//! externally observable contract: no stray temp file is left behind, and
//! the final file always contains complete, valid JSON.

use std::path::PathBuf;

use assert_cmd::Command;

#[allow(clippy::expect_used)]
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tooned"))
        .parent()
        .expect("compiled binary has a parent directory")
        .to_path_buf()
}

#[test]
fn claude_code_install_leaves_no_stray_temp_files() {
    let home = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--claude-code", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success();

    let claude_dir = home.path().join(".claude");
    let entries: Vec<String> = std::fs::read_dir(&claude_dir)
        .expect("read .claude dir")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["settings.json".to_string()], "no stray temp files, got {entries:?}");

    let contents =
        std::fs::read_to_string(claude_dir.join("settings.json")).expect("read settings.json");
    let _: serde_json::Value =
        serde_json::from_str(&contents).expect("final file is complete, valid json");
}

/// T071 edge case, exercised for real (spec.md Edge Cases: "What happens if
/// two installers ... attempt to modify agent configuration at the same
/// time?"): launches several `tooned hook install` invocations
/// concurrently against the *same* target `settings.json`, actually racing
/// the temp-file-then-rename path against itself (a `Barrier` lines all
/// threads up to start their child process near-simultaneously) rather than
/// only ever exercising one installer run at a time sequentially. Asserts
/// both halves of the atomicity guarantee: no stray temp file survives the
/// race, and the final file is complete/valid JSON containing tooned's
/// entry exactly once (not duplicated, truncated, or interleaved).
#[test]
fn concurrent_claude_code_installs_leave_no_stray_temp_files_and_valid_json() {
    const WRITERS: usize = 8;

    let home = tempfile::tempdir().expect("tempdir");
    let home_path = home.path().to_path_buf();
    let path_env = bin_dir();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(WRITERS));

    let handles: Vec<_> = (0..WRITERS)
        .map(|_| {
            let home_path = home_path.clone();
            let path_env = path_env.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                Command::cargo_bin("tooned")
                    .expect("binary exists")
                    .args(["hook", "install", "--claude-code", "--scope", "user"])
                    .env_clear()
                    .env("PATH", &path_env)
                    .env("HOME", &home_path)
                    .assert()
                    .success();
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("installer thread must not panic");
    }

    let claude_dir = home_path.join(".claude");
    let entries: Vec<String> = std::fs::read_dir(&claude_dir)
        .expect("read .claude dir")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec!["settings.json".to_string()],
        "no stray temp files must survive a concurrent-writer race, got {entries:?}"
    );

    let contents =
        std::fs::read_to_string(claude_dir.join("settings.json")).expect("read settings.json");
    let value: serde_json::Value = serde_json::from_str(&contents)
        .expect("final file must be complete, valid JSON even after a concurrent-writer race");

    let post_tool_use = value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array must be present after a successful install");
    let tooned_entries: Vec<_> = post_tool_use
        .iter()
        .filter(|entry| {
            entry.get("hooks").and_then(serde_json::Value::as_array).is_some_and(|inner| {
                inner.iter().any(|h| {
                    h.get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|c| c.ends_with("hook run --claude-code"))
                })
            })
        })
        .collect();
    assert_eq!(
        tooned_entries.len(),
        1,
        "concurrent installer runs targeting the same file must converge on exactly one \
         tooned entry, not duplicate or corrupt it: got {tooned_entries:?}"
    );
}

/// Same race as above, for the Codex plugin bundle's `hooks/hooks.json`
/// (a distinct write path from Claude Code's `settings.json` --
/// `codex::install` writes into `.codex-plugin/` in the current directory
/// rather than `$HOME/.claude/`).
#[test]
fn concurrent_codex_installs_leave_no_stray_temp_files_and_valid_json() {
    const WRITERS: usize = 8;

    let project = tempfile::tempdir().expect("tempdir");
    let project_path = project.path().to_path_buf();
    let path_env = bin_dir();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(WRITERS));

    let handles: Vec<_> = (0..WRITERS)
        .map(|_| {
            let project_path = project_path.clone();
            let path_env = path_env.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                Command::cargo_bin("tooned")
                    .expect("binary exists")
                    .args(["hook", "install", "--codex"])
                    .env_clear()
                    .env("PATH", &path_env)
                    .current_dir(&project_path)
                    .assert()
                    .success();
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("installer thread must not panic");
    }

    let hooks_dir = project_path.join(".codex-plugin").join("hooks");
    let entries: Vec<String> = std::fs::read_dir(&hooks_dir)
        .expect("read hooks dir")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec!["hooks.json".to_string()],
        "no stray temp files must survive a concurrent-writer race, got {entries:?}"
    );

    let contents = std::fs::read_to_string(hooks_dir.join("hooks.json")).expect("read hooks.json");
    let value: serde_json::Value = serde_json::from_str(&contents)
        .expect("final file must be complete, valid JSON even after a concurrent-writer race");
    let post_tool_use = value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array must be present after a successful install");
    let tooned_entries: Vec<_> = post_tool_use
        .iter()
        .filter(|entry| {
            entry.get("hooks").and_then(serde_json::Value::as_array).is_some_and(|inner| {
                inner.iter().any(|h| {
                    h.get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|c| c.ends_with("hook run --codex"))
                })
            })
        })
        .collect();
    assert_eq!(
        tooned_entries.len(),
        1,
        "concurrent installer runs targeting the same file must converge on exactly one \
         tooned entry, not duplicate or corrupt it: got {tooned_entries:?}"
    );
}

#[test]
fn codex_install_leaves_no_stray_temp_files() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--codex"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_dir = project.path().join(".codex-plugin").join("hooks");
    let entries: Vec<String> = std::fs::read_dir(&hooks_dir)
        .expect("read hooks dir")
        .map(|e| e.expect("dir entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(entries, vec!["hooks.json".to_string()], "no stray temp files, got {entries:?}");
}
