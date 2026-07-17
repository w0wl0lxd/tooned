// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Integration tests for `tooned hook install --claude-code` (T028, T029).
//! See `specs/001-adaptive-toon-conversion/contracts/claude-code-hook.md`.

use std::path::PathBuf;

use assert_cmd::Command;

/// Directory containing the freshly-built `tooned` binary under test
/// (`CARGO_BIN_EXE_tooned` is set by Cargo for integration tests of a
/// package with a matching `[[bin]]` target). Used to build a minimal `PATH`
/// so the installer's own PATH-resolution logic finds a real `tooned`
/// executable without depending on anything already installed on the host.
// `expect`/`unwrap` are allowed in `#[test]` function bodies by
// clippy.toml's allow-*-in-tests config, but that detection is lexical (it
// doesn't reach free helper functions only ever called from tests), so
// these two deliberately opt back in -- same rationale, just spelled out
// explicitly per clippy.toml's own stated policy for scoped exceptions.
#[allow(clippy::expect_used)]
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tooned"))
        .parent()
        .expect("compiled binary has a parent directory")
        .to_path_buf()
}

#[allow(clippy::expect_used)]
fn post_tool_use_entries(settings_json: &serde_json::Value) -> Vec<serde_json::Value> {
    settings_json
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present")
        .clone()
}

#[test]
fn install_writes_expected_matcher_and_command() {
    let home = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--claude-code", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success();

    let settings_path = home.path().join(".claude").join("settings.json");
    let contents = std::fs::read_to_string(&settings_path).expect("settings.json written");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");

    let entries = post_tool_use_entries(&value);
    assert_eq!(entries.len(), 1, "exactly one entry after a single install");

    let entry = entries.first().expect("entry present");
    assert_eq!(
        entry.get("matcher").and_then(serde_json::Value::as_str),
        Some("Bash|Read|Grep|WebFetch|^mcp__"),
        "matcher must be exactly the contract's regex, entry was {entry}"
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
    // the exact suffix (`hooks::CLAUDE_CODE_COMMAND_SUFFIX`) the installer
    // itself uses for idempotency/uninstall detection.
    assert!(
        command.ends_with("hook run --claude-code"),
        "command must invoke `hook run --claude-code`, got {command:?}"
    );
    assert!(
        command.to_ascii_lowercase().contains("tooned"),
        "command must invoke the tooned binary, got {command:?}"
    );
}

#[test]
fn install_run_twice_does_not_duplicate_entry() {
    let home = tempfile::tempdir().expect("tempdir");

    let run_install = || {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "install", "--claude-code", "--scope", "user"])
            .env_clear()
            .env("PATH", bin_dir())
            .env("HOME", home.path())
            .assert()
            .success();
    };
    run_install();
    run_install();

    let settings_path = home.path().join(".claude").join("settings.json");
    let contents = std::fs::read_to_string(&settings_path).expect("settings.json written");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");

    let entries = post_tool_use_entries(&value);
    assert_eq!(entries.len(), 1, "installing twice must not duplicate the entry");
}

#[test]
fn install_preserves_pre_existing_unrelated_settings() {
    let home = tempfile::tempdir().expect("tempdir");
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("create .claude dir");
    std::fs::write(
        claude_dir.join("settings.json"),
        serde_json::json!({"someOtherSetting": true}).to_string(),
    )
    .expect("write pre-existing settings.json");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--claude-code", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success();

    let contents = std::fs::read_to_string(claude_dir.join("settings.json"))
        .expect("settings.json still present");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    assert_eq!(value.get("someOtherSetting"), Some(&serde_json::Value::Bool(true)));
    assert_eq!(post_tool_use_entries(&value).len(), 1);
}
