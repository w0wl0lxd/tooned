// SPDX-License-Identifier: AGPL-3.0-only

//! Integration tests for `tooned completions`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use assert_cmd::Command;

fn completions(shell: &str) -> String {
    let output = Command::cargo_bin("tooned")
        .unwrap()
        .args(["completions", "--shell", shell])
        .output()
        .unwrap();
    assert!(output.status.success(), "completions {shell} failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn completions_bash_contains_function() {
    assert!(completions("bash").contains("_tooned"));
}

#[test]
fn completions_zsh_contains_compdef() {
    assert!(completions("zsh").contains("#compdef tooned"));
}

#[test]
fn completions_fish_contains_complete() {
    assert!(completions("fish").contains("complete -c tooned"));
}

#[test]
fn completions_powershell_contains_register() {
    assert!(completions("powershell").contains("Register-ArgumentCompleter"));
}

#[test]
fn completions_elvish_contains_builtin() {
    assert!(completions("elvish").contains("use builtin;"));
}
