// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for `tooned hook install --codex [--mcp]`
//! (T031, T031b). See
//! `specs/001-adaptive-toon-conversion/contracts/codex-hook.md`.

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

// See the matching comment in `hook_install_claude_code.rs`: clippy's
// allow-*-in-tests config is lexical and doesn't reach free helper
// functions only ever called from `#[test]` bodies.
#[allow(clippy::expect_used)]
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tooned"))
        .parent()
        .expect("compiled binary has a parent directory")
        .to_path_buf()
}

#[allow(clippy::expect_used)]
fn read_hooks_post_tool_use(hooks_json_path: &std::path::Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(hooks_json_path).expect("read hooks.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present")
        .clone()
}

#[test]
fn install_writes_plugin_bundle_with_bash_matcher_and_prints_trust_review_instruction() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--codex"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("/hooks"));

    let plugin_json_path = project.path().join(".codex-plugin").join("plugin.json");
    let hooks_json_path = project.path().join(".codex-plugin").join("hooks").join("hooks.json");
    let mcp_json_path = project.path().join(".codex-plugin").join(".mcp.json");

    assert!(plugin_json_path.exists(), "plugin.json must be written");
    assert!(hooks_json_path.exists(), "hooks/hooks.json must be written");
    assert!(!mcp_json_path.exists(), ".mcp.json must NOT be written without --mcp");

    let entries = read_hooks_post_tool_use(&hooks_json_path);
    assert_eq!(entries.len(), 1);
    let entry = entries.first().expect("entry present");
    assert_eq!(
        entry.get("matcher").and_then(serde_json::Value::as_str),
        Some("Bash"),
        "Codex matcher must be the literal string \"Bash\", entry was {entry}"
    );
    let command = entry
        .get("hooks")
        .and_then(serde_json::Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|h| h.get("command"))
        .and_then(serde_json::Value::as_str)
        .expect("hooks[0].command string present");
    // Suffix-only match (no "tooned" prefix): on Windows the resolved
    // binary is an absolute path ending in `tooned.exe`, so a literal
    // "tooned hook run --..." suffix never matches there. This mirrors
    // the exact suffix (`hooks::CODEX_COMMAND_SUFFIX`) the installer
    // itself uses for idempotency/uninstall detection.
    assert!(
        command.ends_with("hook run --codex"),
        "command must invoke `hook run --codex`, got {command:?}"
    );
    assert!(
        command.to_ascii_lowercase().contains("tooned"),
        "command must invoke the tooned binary, got {command:?}"
    );
}

#[test]
fn install_with_mcp_writes_mcp_json() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--codex", "--mcp"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let mcp_json_path = project.path().join(".codex-plugin").join(".mcp.json");
    assert!(mcp_json_path.exists(), ".mcp.json must be written when --mcp is passed");
}

#[test]
fn install_run_twice_does_not_duplicate_hook_entry() {
    let project = tempfile::tempdir().expect("tempdir");

    let run_install = || {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "install", "--codex"])
            .env_clear()
            .env("PATH", bin_dir())
            .current_dir(project.path())
            .assert()
            .success();
    };
    run_install();
    run_install();

    let hooks_json_path = project.path().join(".codex-plugin").join("hooks").join("hooks.json");
    let entries = read_hooks_post_tool_use(&hooks_json_path);
    assert_eq!(entries.len(), 1, "installing twice must not duplicate the entry");
}
