// SPDX-License-Identifier: AGPL-3.0-only

//! T080b: `--help` output for the top-level `tooned` command and every
//! subcommand is non-empty and documents its required/key flags (SC-006 --
//! a new developer should be able to use tooned from `--help` alone,
//! without external docs).

use assert_cmd::Command;

/// Runs `tooned <args> --help`, asserts success and non-empty stdout, and
/// asserts every string in `must_contain` appears somewhere in it (the key
/// flags/positionals a user needs to know about for that subcommand).
#[allow(clippy::expect_used)] // test-only helper in an integration-test binary
fn assert_help(args: &[&str], must_contain: &[&str]) {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("--help");

    let output = Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(&full_args)
        .output()
        .expect("run `tooned ... --help`");
    assert!(
        output.status.success(),
        "`tooned {} --help` did not exit successfully: {}",
        full_args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("--help output is valid UTF-8");
    assert!(
        !stdout.trim().is_empty(),
        "`tooned {} --help` produced empty output",
        full_args.join(" ")
    );

    for needle in must_contain {
        assert!(
            stdout.contains(needle),
            "`tooned {} --help` output did not document `{needle}`:\n{stdout}",
            full_args.join(" ")
        );
    }
}

#[test]
fn top_level_help_documents_every_subcommand() {
    assert_help(&[], &["convert", "check", "pipe", "wrap", "index", "stats", "hook", "mcp"]);
}

#[test]
fn convert_help_documents_input_and_direction_flags() {
    assert_help(&["convert"], &["INPUT", "--to", "--out"]);
}

#[test]
fn check_help_documents_input_and_precise_flag() {
    assert_help(&["check"], &["INPUT", "--precise"]);
}

#[test]
fn pipe_help_documents_margin_and_max_bytes_flags() {
    assert_help(&["pipe"], &["--margin", "--max-bytes"]);
}

#[test]
fn wrap_help_documents_the_trailing_command() {
    assert_help(&["wrap"], &["COMMAND"]);
}

#[test]
fn index_help_documents_the_path_and_subcommands() {
    assert_help(&["index"], &["PATH", "sync", "status", "show"]);
}

#[test]
fn index_sync_help_is_non_empty() {
    assert_help(&["index", "sync"], &[]);
}

#[test]
fn index_status_help_is_non_empty() {
    assert_help(&["index", "status"], &[]);
}

#[test]
fn index_show_help_documents_the_required_file_argument() {
    assert_help(&["index", "show"], &["FILE"]);
}

#[test]
fn stats_help_documents_the_top_flag() {
    assert_help(&["stats"], &["--top"]);
}

#[test]
fn hook_help_documents_every_hook_subcommand() {
    assert_help(&["hook"], &["run", "install", "uninstall", "status", "doctor"]);
}

#[test]
fn hook_run_help_documents_the_agent_selector_flags() {
    assert_help(&["hook", "run"], &["--claude-code", "--codex"]);
}

#[test]
fn hook_install_help_documents_agent_scope_and_mcp_flags() {
    assert_help(&["hook", "install"], &["--claude-code", "--codex", "--scope", "--mcp"]);
}

#[test]
fn hook_uninstall_help_documents_agent_and_scope_flags() {
    assert_help(&["hook", "uninstall"], &["--claude-code", "--codex", "--scope"]);
}

#[test]
fn hook_status_help_documents_the_agent_selector_flags() {
    assert_help(&["hook", "status"], &["--claude-code", "--codex"]);
}

#[test]
fn hook_doctor_help_is_non_empty() {
    assert_help(&["hook", "doctor"], &[]);
}

#[test]
fn mcp_help_documents_the_serve_subcommand() {
    assert_help(&["mcp"], &["serve"]);
}

#[test]
fn mcp_serve_help_is_non_empty() {
    assert_help(&["mcp", "serve"], &[]);
}
