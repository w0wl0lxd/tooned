// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Integration tests for `tooned config`.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn config_init_writes_loadable_default_config() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("tooned.toml");

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["config", "init", "--out", out.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["config", "validate", "--config", out.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn config_validate_rejects_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tooned.toml");
    std::fs::write(&path, "unknown_key = true\n").unwrap();

    Command::cargo_bin("tooned")
        .unwrap()
        .args(["config", "validate", "--config", path.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown"));
}
