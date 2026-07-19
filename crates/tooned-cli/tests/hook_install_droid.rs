// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Integration tests for `tooned hook install --droid` and `uninstall --droid`.

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
fn read_hooks(path: &std::path::Path) -> Vec<serde_json::Value> {
    let contents = std::fs::read_to_string(path).expect("read hooks.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present")
        .clone()
}

#[allow(clippy::expect_used)]
fn assert_tooned_entry(entry: &serde_json::Value) {
    assert_eq!(
        entry.get("matcher").and_then(serde_json::Value::as_str),
        Some("Execute|Read|Grep|Glob|FetchUrl|WebSearch|^mcp__"),
        "matcher must be the Droid contract regex, got {entry}"
    );

    let inner = entry
        .get("hooks")
        .and_then(serde_json::Value::as_array)
        .and_then(|arr| arr.first())
        .expect("hooks[0] object present");

    let command = inner
        .get("command")
        .and_then(serde_json::Value::as_str)
        .expect("hooks[0].command string present");
    assert!(
        command.ends_with("hook run --droid"),
        "command must invoke `hook run --droid`, got {command:?}"
    );

    assert_eq!(
        inner.get("timeout").and_then(serde_json::Value::as_u64),
        Some(5),
        "generated hook must include a 5-second timeout, got {inner}"
    );
}

#[test]
fn install_project_scope_writes_factory_hooks_json() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--droid"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_path = project.path().join(".factory").join("hooks.json");
    assert!(hooks_path.exists(), ".factory/hooks.json must be written");

    let entries = read_hooks(&hooks_path);
    assert_eq!(entries.len(), 1);
    assert_tooned_entry(entries.first().expect("entry present"));
}

#[test]
fn install_user_scope_writes_factory_hooks_json() {
    let home = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--droid", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success();

    let hooks_path = home.path().join(".factory").join("hooks.json");
    assert!(hooks_path.exists(), "~/.factory/hooks.json must be written");

    let entries = read_hooks(&hooks_path);
    assert_eq!(entries.len(), 1);
    assert_tooned_entry(entries.first().expect("entry present"));
}

#[test]
fn install_run_twice_does_not_duplicate_hook_entry() {
    let project = tempfile::tempdir().expect("tempdir");

    let run_install = || {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "install", "--droid"])
            .env_clear()
            .env("PATH", bin_dir())
            .current_dir(project.path())
            .assert()
            .success();
    };
    run_install();
    run_install();

    let hooks_path = project.path().join(".factory").join("hooks.json");
    let entries = read_hooks(&hooks_path);
    assert_eq!(entries.len(), 1, "installing twice must not duplicate the entry");
}

#[test]
fn uninstall_removes_droid_hook_entry() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--droid"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "uninstall", "--droid"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_path = project.path().join(".factory").join("hooks.json");
    let contents = std::fs::read_to_string(&hooks_path).expect("read hooks.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    let entries = value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present");
    assert!(entries.is_empty(), "uninstall must remove the tooned entry");
}

#[test]
fn install_project_scope_preserves_foreign_entry_and_appends_own() {
    let project = tempfile::tempdir().expect("tempdir");
    let factory_dir = project.path().join(".factory");
    std::fs::create_dir_all(&factory_dir).expect("mkdir .factory");

    let foreign_entry = serde_json::json!({
        "matcher": "Execute",
        "hooks": [ { "type": "command", "command": "/usr/local/bin/rtk hook run" } ],
    });
    let seeded = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry ] } });
    std::fs::write(
        factory_dir.join("hooks.json"),
        serde_json::to_string_pretty(&seeded).expect("ser"),
    )
    .expect("write seeded hooks.json");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--droid"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_path = project.path().join(".factory").join("hooks.json");
    let entries = read_hooks(&hooks_path);
    assert_eq!(
        entries.len(),
        2,
        "foreign entry preserved and tooned's own appended, got {entries:?}"
    );
    assert!(entries.contains(&foreign_entry), "foreign entry must be structurally unchanged");
    assert!(entries.iter().any(|e| {
        e.get("hooks").and_then(serde_json::Value::as_array).is_some_and(|inner| {
            inner.iter().any(|h| {
                h.get("command")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|c| c.ends_with("hook run --droid"))
            })
        })
    }));
}

#[test]
fn install_project_scope_coerces_malformed_root_to_object() {
    let project = tempfile::tempdir().expect("tempdir");
    let factory_dir = project.path().join(".factory");
    std::fs::create_dir_all(&factory_dir).expect("mkdir .factory");

    std::fs::write(factory_dir.join("hooks.json"), "[]").expect("write malformed hooks.json");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--droid"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let hooks_path = project.path().join(".factory").join("hooks.json");
    let entries = read_hooks(&hooks_path);
    assert_eq!(entries.len(), 1, "install must replace a malformed root with a valid object");
    assert_tooned_entry(entries.first().expect("entry present"));
}
