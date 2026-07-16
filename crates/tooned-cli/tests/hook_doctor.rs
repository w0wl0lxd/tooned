// SPDX-License-Identifier: AGPL-3.0-only

//! Contract test (T066): `tooned hook doctor` reports both tooned's own
//! and a foreign tool's hook entries correctly, across both agents' configs,
//! and performs no writes to either config file.
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md` and
//! data-model.md ("`tooned hook doctor` reads (never writes)...").

use std::path::PathBuf;

use assert_cmd::Command;

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

#[test]
fn doctor_reports_tooned_and_foreign_entries_and_never_writes() {
    let home = tempfile::tempdir().expect("tempdir");
    let project = tempfile::tempdir().expect("tempdir");

    let claude_dir = home.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).expect("mkdir .claude");
    let seeded_claude = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry() ] } });
    let claude_settings_path = claude_dir.join("settings.json");
    std::fs::write(
        &claude_settings_path,
        serde_json::to_string_pretty(&seeded_claude).expect("ser"),
    )
    .expect("write seeded settings.json");

    let codex_hooks_dir = project.path().join(".codex-plugin").join("hooks");
    std::fs::create_dir_all(&codex_hooks_dir).expect("mkdir .codex-plugin/hooks");
    let seeded_codex = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry() ] } });
    let codex_hooks_path = codex_hooks_dir.join("hooks.json");
    std::fs::write(&codex_hooks_path, serde_json::to_string_pretty(&seeded_codex).expect("ser"))
        .expect("write seeded hooks.json");

    let run = |args: &[&str]| {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(args)
            .env_clear()
            .env("PATH", bin_dir())
            .env("HOME", home.path())
            .current_dir(project.path())
            .assert()
    };
    run(&["hook", "install", "--claude-code", "--scope", "user"]).success();
    run(&["hook", "install", "--codex"]).success();

    let claude_before = std::fs::read(&claude_settings_path).expect("read claude before");
    let codex_before = std::fs::read(&codex_hooks_path).expect("read codex before");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "doctor"])
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .current_dir(project.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let claude_after = std::fs::read(&claude_settings_path).expect("read claude after");
    let codex_after = std::fs::read(&codex_hooks_path).expect("read codex after");
    assert_eq!(claude_before, claude_after, "doctor must never write to Claude Code's config");
    assert_eq!(codex_before, codex_after, "doctor must never write to Codex's config");

    let report_text = String::from_utf8(output).expect("utf8 output");
    assert!(
        report_text.contains("rtk"),
        "must mention the foreign tool's command, got: {report_text}"
    );
    assert!(
        report_text.contains("hook run --claude-code"),
        "must mention tooned's own Claude Code command, got: {report_text}"
    );
    assert!(
        report_text.contains("hook run --codex"),
        "must mention tooned's own Codex command, got: {report_text}"
    );
}
