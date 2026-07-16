// SPDX-License-Identifier: AGPL-3.0-only

//! Contract test (T067): `tooned hook status (--claude-code|--codex)`
//! correctly reports installed vs. not-installed.
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md` ("0 always").

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

#[test]
fn claude_code_status_not_installed_before_install() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "status", "--claude-code"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("not installed"));
}

#[test]
fn claude_code_status_installed_after_install() {
    let project = tempfile::tempdir().expect("tempdir");

    let run = |args: &[&str]| {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(args)
            .env_clear()
            .env("PATH", bin_dir())
            .current_dir(project.path())
            .assert()
    };
    run(&["hook", "install", "--claude-code"]).success();
    run(&["hook", "status", "--claude-code"])
        .success()
        .stdout(predicate::str::contains("is installed"));
}

#[test]
fn codex_status_not_installed_before_install() {
    let project = tempfile::tempdir().expect("tempdir");

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "status", "--codex"])
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("not installed"));
}

#[test]
fn codex_status_installed_after_install() {
    let project = tempfile::tempdir().expect("tempdir");

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
    run(&["hook", "status", "--codex"]).success().stdout(predicate::str::contains("is installed"));
}
