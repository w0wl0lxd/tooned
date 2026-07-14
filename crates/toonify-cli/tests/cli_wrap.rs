//! Contract test for `tooned wrap` (T043).
//! See `specs/001-adaptive-toon-conversion/contracts/cli.md`.

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

    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["wrap", "--", "printf", "%s", &json])
        .assert()
        .success()
        .stdout(predicate::str::contains("id,name,active,score"));
}

#[test]
fn wrap_mirrors_a_nonzero_exit_code() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["wrap", "--", "sh", "-c", "echo not-json; exit 7"])
        .assert()
        .code(7)
        .stdout(predicate::str::contains("not-json"));
}

#[test]
fn wrap_passes_wrapped_stderr_through_unchanged() {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["wrap", "--", "sh", "-c", "echo err-message 1>&2"])
        .assert()
        .success()
        .stderr(predicate::str::contains("err-message"));
}
