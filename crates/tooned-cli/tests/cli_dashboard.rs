// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned dashboard` integration tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn dashboard_help_documents_tui_flags() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["dashboard", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--global").or(predicate::str::contains("--metric")));
}
