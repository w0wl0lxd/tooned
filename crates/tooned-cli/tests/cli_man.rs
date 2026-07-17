// SPDX-License-Identifier: AGPL-3.0-only

//! Integration test for `tooned man`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;

#[test]
fn man_emits_roff_page() {
    let output = Command::cargo_bin("tooned")
        .unwrap()
        .arg("man")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(".TH tooned"));
    assert!(stdout.contains(".SH NAME"));
}
