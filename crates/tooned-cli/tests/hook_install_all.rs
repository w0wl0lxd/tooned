// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Integration tests for `tooned hook install --all`.

use std::path::PathBuf;

use assert_cmd::Command;

#[allow(clippy::expect_used)]
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tooned"))
        .parent()
        .expect("compiled binary has a parent directory")
        .to_path_buf()
}

#[allow(clippy::expect_used)]
fn read_project_hooks(path: &std::path::Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(path).expect("read hooks file");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    value
        .get("PostToolUse")
        .and_then(serde_json::Value::as_array)
        .expect("top-level PostToolUse array present")
        .clone()
}

#[allow(clippy::expect_used)]
fn read_nested_hooks(path: &std::path::Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(path).expect("read hooks file");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present")
        .clone()
}

#[test]
fn install_all_project_scope_writes_hooks_for_all_agents() {
    let project = tempfile::tempdir().expect("tempdir");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--all"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .output()
        .expect("run `hook install --all`");

    assert!(
        output.status.success(),
        "`hook install --all` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("installed hooks for claude-code, codex, devin, droid, opencode, kilo, pi"),
        "expected summary of all installed agents, got {stdout}"
    );

    // Devin project hook (root-level PostToolUse)
    let devin_hooks = project.path().join(".devin").join("hooks.v1.json");
    assert!(devin_hooks.exists(), ".devin/hooks.v1.json must be written");
    assert!(
        !read_project_hooks(&devin_hooks).is_empty(),
        "Devin PostToolUse entries must be present"
    );

    // Claude Code project hook (nested under hooks.PostToolUse)
    let claude_settings = project.path().join(".claude").join("settings.json");
    assert!(claude_settings.exists(), ".claude/settings.json must be written");
    assert!(
        !read_nested_hooks(&claude_settings).is_empty(),
        "Claude Code PostToolUse entries must be present"
    );

    // Codex project hook (nested under hooks.PostToolUse)
    let codex_hooks = project
        .path()
        .join(".codex-plugin")
        .join("hooks")
        .join("hooks.json");
    assert!(codex_hooks.exists(), ".codex-plugin/hooks/hooks.json must be written");
    assert!(
        !read_nested_hooks(&codex_hooks).is_empty(),
        "Codex PostToolUse entries must be present"
    );
}

#[test]
fn install_all_is_idempotent() {
    let project = tempfile::tempdir().expect("tempdir");

    let run_install = || {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "install", "--all"])
            .env_clear()
            .env("PATH", bin_dir())
            .current_dir(project.path())
            .assert()
            .success();
    };
    run_install();
    run_install();

    let devin_hooks = project.path().join(".devin").join("hooks.v1.json");
    let entries = read_project_hooks(&devin_hooks);
    assert_eq!(
        entries.len(),
        1,
        "installing --all twice must not duplicate the Devin entry"
    );
}

#[test]
fn uninstall_all_removes_hooks_for_all_agents() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--all"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "uninstall", "--all"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let devin_hooks = project.path().join(".devin").join("hooks.v1.json");
    let contents = std::fs::read_to_string(&devin_hooks).expect("read hooks.v1.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    let entries = value
        .get("PostToolUse")
        .and_then(serde_json::Value::as_array)
        .expect("PostToolUse array present");
    assert!(entries.is_empty(), "uninstall --all must remove the Devin entry");
}
