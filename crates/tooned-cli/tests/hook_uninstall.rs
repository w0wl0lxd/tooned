//! Uninstall tests (T064, T065): `tooned hook uninstall` removes only
//! tooned's own entry, leaves a foreign entry untouched, and reports
//! "nothing to remove" (without erroring) when tooned was never installed.
//! Covers both Claude Code and Codex, per `specs/001-adaptive-toon-conversion/
//! contracts/{claude-code-hook,codex-hook}.md` and data-model.md's
//! "Integration Installation Record" rules (FR-018).

use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

#[allow(clippy::expect_used)]
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tooned"))
        .parent()
        .expect("compiled binary has a parent directory")
        .to_path_buf()
}

fn foreign_entry() -> serde_json::Value {
    serde_json::json!({
        "matcher": "Bash",
        "hooks": [ { "type": "command", "command": "/usr/local/bin/rtk hook run" } ],
    })
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
fn claude_code_uninstall_removes_only_own_entry() {
    let home = tempfile::tempdir().expect("tempdir");
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("mkdir .claude");
    let seeded = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry() ] } });
    std::fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&seeded).expect("ser"),
    )
    .expect("write seeded settings.json");

    let run = |args: &[&str]| {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(args)
            .env_clear()
            .env("PATH", bin_dir())
            .env("HOME", home.path())
            .assert()
    };
    run(&["hook", "install", "--claude-code", "--scope", "user"]).success();

    let settings_path = claude_dir.join("settings.json");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).expect("read"))
            .expect("json");
    assert_eq!(
        post_tool_use_entries(&value).len(),
        2,
        "sanity: foreign + tooned's own present before uninstall"
    );

    run(&["hook", "uninstall", "--claude-code", "--scope", "user"])
        .success()
        .stdout(predicate::str::contains("removed"));

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&settings_path).expect("read"))
            .expect("json");
    let entries = post_tool_use_entries(&value);
    assert_eq!(entries.len(), 1, "only tooned's own entry must be removed, got {entries:?}");
    assert_eq!(entries.first(), Some(&foreign_entry()), "foreign entry must remain intact");
}

#[test]
fn claude_code_uninstall_when_never_installed_reports_nothing_to_remove() {
    let home = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "uninstall", "--claude-code", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to remove"));
}

#[test]
fn claude_code_uninstall_never_installed_but_foreign_entry_present_leaves_it_intact() {
    let home = tempfile::tempdir().expect("tempdir");
    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("mkdir .claude");
    let seeded = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry() ] } });
    let settings_path = claude_dir.join("settings.json");
    std::fs::write(&settings_path, serde_json::to_string_pretty(&seeded).expect("ser"))
        .expect("write seeded settings.json");
    let before = std::fs::read(&settings_path).expect("read before");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "uninstall", "--claude-code", "--scope", "user"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to remove"));

    let after = std::fs::read(&settings_path).expect("read after");
    let before_value: serde_json::Value = serde_json::from_slice(&before).expect("json");
    let after_value: serde_json::Value = serde_json::from_slice(&after).expect("json");
    assert_eq!(
        before_value, after_value,
        "foreign-only config must be left untouched by a no-op uninstall"
    );
}

#[test]
fn codex_uninstall_removes_only_own_entry() {
    let project = tempfile::tempdir().expect("tempdir");
    let hooks_dir = project.path().join(".codex-plugin").join("hooks");
    std::fs::create_dir_all(&hooks_dir).expect("mkdir .codex-plugin/hooks");
    let seeded = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry() ] } });
    let hooks_json_path = hooks_dir.join("hooks.json");
    std::fs::write(&hooks_json_path, serde_json::to_string_pretty(&seeded).expect("ser"))
        .expect("write seeded hooks.json");

    let run = |args: &[&str]| {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(args)
            .env_clear()
            .env("PATH", bin_dir())
            .current_dir(project.path())
            .assert()
    };
    run(&["hook", "install", "--codex"]).success();

    run(&["hook", "uninstall", "--codex"]).success().stdout(predicate::str::contains("removed"));

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&hooks_json_path).expect("read"))
            .expect("json");
    let entries = post_tool_use_entries(&value);
    assert_eq!(entries.len(), 1, "only tooned's own entry must be removed, got {entries:?}");
    assert_eq!(entries.first(), Some(&foreign_entry()), "foreign entry must remain intact");
}

#[test]
fn codex_uninstall_when_never_installed_reports_nothing_to_remove() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "uninstall", "--codex"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing to remove"));

    assert!(
        !project.path().join(".codex-plugin").exists(),
        "no-op uninstall must not create a .codex-plugin/ bundle"
    );
}
