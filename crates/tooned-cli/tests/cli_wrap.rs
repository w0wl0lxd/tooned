// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract test for `tooned wrap` (T043).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.
//!
//! The wrapped commands are cross-platform: `sh`/`printf` aren't guaranteed
//! to exist on a native Windows runner (no shell metacharacter parsing is
//! involved either way, since `tooned wrap -- <argv>` execs its argv
//! directly rather than through a shell).

use std::fmt::Write as _;

use assert_cmd::Command;
use predicates::prelude::*;

fn uniform_array_json(rows: usize) -> String {
    let mut s = String::from("[");
    for i in 0..rows {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, r#"{{"id":{i},"name":"row-{i}","active":true,"score":{i}.5}}"#);
    }
    s.push(']');
    s
}

#[test]
fn wrap_converts_captured_stdout_and_mirrors_exit_code() {
    let json = uniform_array_json(20);
    let mut cmd = Command::cargo_bin("tooned").expect("binary exists");

    if cfg!(windows) {
        // `tooned wrap -- <argv>` execs its argv directly (no shell), so
        // the JSON travels through an env var rather than a `-Command`
        // string -- avoids PowerShell needing to re-parse `{`/`"`/`,` as
        // command-line syntax.
        cmd.env("TOONED_TEST_TEXT", &json).args([
            "wrap",
            "--",
            "powershell",
            "-NoProfile",
            "-Command",
            "[Console]::Out.Write($env:TOONED_TEST_TEXT)",
        ]);
    } else {
        // Passed as a literal argv element (no shell involved), so `printf`
        // never has to parse `json`'s contents at all.
        cmd.args(["wrap", "--", "printf", "%s", &json]);
    }

    cmd.assert().success().stdout(predicate::str::contains("id,name,active,score"));
}

#[test]
fn wrap_mirrors_a_nonzero_exit_code() {
    let mut cmd = Command::cargo_bin("tooned").expect("binary exists");
    if cfg!(windows) {
        cmd.args([
            "wrap",
            "--",
            "powershell",
            "-NoProfile",
            "-Command",
            "Write-Output 'not-json'; exit 7",
        ]);
    } else {
        cmd.args(["wrap", "--", "sh", "-c", "echo not-json; exit 7"]);
    }
    cmd.assert().code(7).stdout(predicate::str::contains("not-json"));
}

#[test]
fn wrap_passes_wrapped_stderr_through_unchanged() {
    let mut cmd = Command::cargo_bin("tooned").expect("binary exists");
    if cfg!(windows) {
        cmd.args([
            "wrap",
            "--",
            "powershell",
            "-NoProfile",
            "-Command",
            "[Console]::Error.WriteLine('err-message')",
        ]);
    } else {
        cmd.args(["wrap", "--", "sh", "-c", "echo err-message 1>&2"]);
    }
    cmd.assert().success().stderr(predicate::str::contains("err-message"));
}
