// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for `tooned hook install --devin`.

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
    let contents = std::fs::read_to_string(path).expect("read hooks.v1.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    value
        .get("PostToolUse")
        .and_then(serde_json::Value::as_array)
        .expect("top-level PostToolUse array present")
        .clone()
}

#[allow(clippy::expect_used)]
fn read_user_hooks(path: &std::path::Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(path).expect("read config.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present")
        .clone()
}

#[test]
fn install_project_scope_writes_devin_hooks_v1_json() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--devin"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_path = project.path().join(".devin").join("hooks.v1.json");
    assert!(hooks_path.exists(), ".devin/hooks.v1.json must be written");

    let entries = read_project_hooks(&hooks_path);
    assert_eq!(entries.len(), 1);
    let entry = entries.first().expect("entry present");
    let command = entry
        .get("hooks")
        .and_then(serde_json::Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|h| h.get("command"))
        .and_then(serde_json::Value::as_str)
        .expect("hooks[0].command string present");
    assert!(
        command.ends_with("hook run --devin"),
        "command must invoke `hook run --devin`, got {command:?}"
    );
}

#[test]
fn install_user_scope_writes_devin_config_json() {
    let home = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--devin", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success();

    let config_path = home.path().join(".config").join("devin").join("config.json");
    assert!(config_path.exists(), "~/.config/devin/config.json must be written");

    let entries = read_user_hooks(&config_path);
    assert_eq!(entries.len(), 1);
}

#[test]
fn install_run_twice_does_not_duplicate_hook_entry() {
    let project = tempfile::tempdir().expect("tempdir");

    let run_install = || {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "install", "--devin"])
            .env_clear()
            .env("PATH", bin_dir())
            .current_dir(project.path())
            .assert()
            .success();
    };
    run_install();
    run_install();

    let hooks_path = project.path().join(".devin").join("hooks.v1.json");
    let entries = read_project_hooks(&hooks_path);
    assert_eq!(entries.len(), 1, "installing twice must not duplicate the entry");
}

#[test]
fn uninstall_removes_devin_hook_entry() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--devin"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "uninstall", "--devin"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_path = project.path().join(".devin").join("hooks.v1.json");
    let contents = std::fs::read_to_string(&hooks_path).expect("read hooks.v1.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    let entries = value
        .get("PostToolUse")
        .and_then(serde_json::Value::as_array)
        .expect("PostToolUse array present");
    assert!(entries.is_empty(), "uninstall must remove the tooned entry");
}
