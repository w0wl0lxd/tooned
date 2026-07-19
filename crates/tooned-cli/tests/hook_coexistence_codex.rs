// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Coexistence test (T063): installing tooned's Codex hook alongside a
//! pre-existing foreign `hooks/hooks.json` entry must leave the foreign
//! entry structurally unchanged and simply append tooned's own.
//! See `specs/001-adaptive-toon-conversion/contracts/codex-hook.md` and
//! constitution Principle V (safe coexistence with e.g. rtk).

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
fn install_preserves_foreign_entry_and_appends_own() {
    let project = tempfile::tempdir().expect("tempdir");
    let hooks_dir = project.path().join(".codex-plugin").join("hooks");
    std::fs::create_dir_all(&hooks_dir).expect("mkdir .codex-plugin/hooks");

    let foreign_entry = serde_json::json!({
        "matcher": "Bash",
        "hooks": [ { "type": "command", "command": "/usr/local/bin/rtk hook run" } ],
    });
    let seeded = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry ] } });
    std::fs::write(
        hooks_dir.join("hooks.json"),
        serde_json::to_string_pretty(&seeded).expect("ser"),
    )
    .expect("write seeded hooks.json");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--codex"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success();

    let contents = std::fs::read_to_string(hooks_dir.join("hooks.json")).expect("read hooks.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    let entries = value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present");

    assert_eq!(
        entries.len(),
        2,
        "foreign entry preserved and tooned's own appended, got {entries:?}"
    );
    assert!(
        entries.contains(&foreign_entry),
        "foreign entry must be structurally unchanged, got {entries:?}"
    );
    assert!(
        entries.iter().any(|e| {
            e.get("hooks").and_then(serde_json::Value::as_array).is_some_and(|inner| {
                inner.iter().any(|h| {
                    h.get("command")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|c| c.ends_with("hook run --codex"))
                })
            })
        }),
        "tooned's own entry must be present, got {entries:?}"
    );
}

#[test]
fn install_run_twice_still_preserves_foreign_entry() {
    let project = tempfile::tempdir().expect("tempdir");
    let hooks_dir = project.path().join(".codex-plugin").join("hooks");
    std::fs::create_dir_all(&hooks_dir).expect("mkdir .codex-plugin/hooks");

    let foreign_entry = serde_json::json!({
        "matcher": "Bash",
        "hooks": [ { "type": "command", "command": "/usr/local/bin/rtk hook run" } ],
    });
    let seeded = serde_json::json!({ "hooks": { "PostToolUse": [ foreign_entry ] } });
    std::fs::write(
        hooks_dir.join("hooks.json"),
        serde_json::to_string_pretty(&seeded).expect("ser"),
    )
    .expect("write seeded hooks.json");

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

    let contents = std::fs::read_to_string(hooks_dir.join("hooks.json")).expect("read hooks.json");
    let value: serde_json::Value = serde_json::from_str(&contents).expect("valid json");
    let entries = value
        .get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .expect("hooks.PostToolUse array present");

    assert_eq!(
        entries.len(),
        2,
        "still exactly foreign + tooned's own, no duplication, got {entries:?}"
    );
    assert!(entries.contains(&foreign_entry));
}
