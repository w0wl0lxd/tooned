// SPDX-License-Identifier: AGPL-3.0-only

//! Integration test (T030): `tooned hook install` aborts clearly, without
//! writing any config, when the `tooned` binary cannot be resolved on
//! `PATH`. See `specs/001-adaptive-toon-conversion/contracts/cli.md`
//! ("4 binary not resolvable on PATH").

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn claude_code_install_aborts_with_exit_4_and_writes_nothing() {
    let home = tempfile::tempdir().expect("tempdir");
    let empty_path_dir = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--claude-code", "--scope", "user"])
        .env_clear()
        .env("PATH", empty_path_dir.path())
        .env("HOME", home.path())
        .assert()
        .code(4)
        .stderr(predicate::str::is_empty().not());

    let settings_path = home.path().join(".claude").join("settings.json");
    assert!(
        !settings_path.exists(),
        "no config must be written when the tooned binary can't be resolved on PATH"
    );
}

#[test]
fn codex_install_aborts_with_exit_4_and_writes_nothing() {
    let project = tempfile::tempdir().expect("tempdir");
    let empty_path_dir = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "install", "--codex"])
        .env_clear()
        .env("PATH", empty_path_dir.path())
        .current_dir(project.path())
        .assert()
        .code(4)
        .stderr(predicate::str::is_empty().not());

    assert!(
        !project.path().join(".codex-plugin").exists(),
        "no .codex-plugin/ bundle must be written when tooned can't be resolved on PATH"
    );
}
