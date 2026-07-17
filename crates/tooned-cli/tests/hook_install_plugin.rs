// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Integration tests for plugin-wrapped agent installs (OpenCode, Kilo, Pi).

use std::path::PathBuf;

use assert_cmd::Command;
use assert_cmd::assert::Assert;

#[allow(clippy::expect_used)]
fn bin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_tooned"))
        .parent()
        .expect("compiled binary has a parent directory")
        .to_path_buf()
}

#[allow(clippy::expect_used)]
fn read_plugin(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).expect("read plugin file")
}

fn install(args: &[&str], project: &tempfile::TempDir) -> Assert {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(args)
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success()
}

fn install_user(args: &[&str], home: &tempfile::TempDir) -> Assert {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(args)
        .env_clear()
        .env("PATH", bin_dir())
        .env("HOME", home.path())
        .assert()
        .success()
}

fn uninstall(args: &[&str], project: &tempfile::TempDir) -> Assert {
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(args)
        .env_clear()
        .env("PATH", bin_dir())
        .current_dir(project.path())
        .assert()
        .success()
}

fn assert_tooned_plugin(path: &std::path::Path, run_flag: &str) {
    assert!(path.exists(), "plugin file must be written: {}", path.display());
    let text = read_plugin(path);
    assert!(text.contains(run_flag), "plugin file must invoke `{run_flag}`, got: {text}");
    assert!(text.contains("tooned"), "plugin file must reference tooned, got: {text}");
}

// ---------- OpenCode ----------

#[test]
fn opencode_install_project_scope_writes_plugin() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--opencode"], &project);
    let path = project.path().join(".opencode").join("plugins").join("tooned.ts");
    assert_tooned_plugin(&path, "--opencode");
}

#[test]
fn opencode_install_user_scope_writes_plugin() {
    let home = tempfile::tempdir().expect("tempdir");
    install_user(&["hook", "install", "--opencode", "--scope", "user"], &home);
    let path = home.path().join(".config").join("opencode").join("plugins").join("tooned.ts");
    assert_tooned_plugin(&path, "--opencode");
}

#[test]
fn opencode_install_run_twice_does_not_duplicate() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--opencode"], &project);
    install(&["hook", "install", "--opencode"], &project);
    let path = project.path().join(".opencode").join("plugins").join("tooned.ts");
    let text = read_plugin(&path);
    assert_eq!(
        text.matches("--opencode").count(),
        1,
        "installing twice must overwrite, not duplicate"
    );
}

#[test]
fn opencode_uninstall_removes_plugin() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--opencode"], &project);
    uninstall(&["hook", "uninstall", "--opencode"], &project);
    let path = project.path().join(".opencode").join("plugins").join("tooned.ts");
    assert!(!path.exists(), "uninstall must remove the plugin file");
}

// ---------- Kilo ----------

#[test]
fn kilo_install_project_scope_writes_plugin() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--kilo"], &project);
    let path = project.path().join(".kilo").join("plugin").join("tooned.ts");
    assert_tooned_plugin(&path, "--kilo");
}

#[test]
fn kilo_install_user_scope_writes_plugin() {
    let home = tempfile::tempdir().expect("tempdir");
    install_user(&["hook", "install", "--kilo", "--scope", "user"], &home);
    let path = home.path().join(".config").join("kilo").join("plugin").join("tooned.ts");
    assert_tooned_plugin(&path, "--kilo");
}

#[test]
fn kilo_install_run_twice_does_not_duplicate() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--kilo"], &project);
    install(&["hook", "install", "--kilo"], &project);
    let path = project.path().join(".kilo").join("plugin").join("tooned.ts");
    let text = read_plugin(&path);
    assert_eq!(text.matches("--kilo").count(), 1, "installing twice must overwrite, not duplicate");
}

#[test]
fn kilo_uninstall_removes_plugin() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--kilo"], &project);
    uninstall(&["hook", "uninstall", "--kilo"], &project);
    let path = project.path().join(".kilo").join("plugin").join("tooned.ts");
    assert!(!path.exists(), "uninstall must remove the plugin file");
}

// ---------- Pi ----------

#[test]
fn pi_install_project_scope_writes_plugin() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--pi"], &project);
    let path = project.path().join(".pi").join("extensions").join("tooned.ts");
    assert_tooned_plugin(&path, "--pi");
}

#[test]
fn pi_install_user_scope_writes_plugin() {
    let home = tempfile::tempdir().expect("tempdir");
    install_user(&["hook", "install", "--pi", "--scope", "user"], &home);
    let path = home.path().join(".pi").join("agent").join("extensions").join("tooned.ts");
    assert_tooned_plugin(&path, "--pi");
}

#[test]
fn pi_install_run_twice_does_not_duplicate() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--pi"], &project);
    install(&["hook", "install", "--pi"], &project);
    let path = project.path().join(".pi").join("extensions").join("tooned.ts");
    let text = read_plugin(&path);
    assert_eq!(text.matches("--pi").count(), 1, "installing twice must overwrite, not duplicate");
}

#[test]
fn pi_uninstall_removes_plugin() {
    let project = tempfile::tempdir().expect("tempdir");
    install(&["hook", "install", "--pi"], &project);
    uninstall(&["hook", "uninstall", "--pi"], &project);
    let path = project.path().join(".pi").join("extensions").join("tooned.ts");
    assert!(!path.exists(), "uninstall must remove the plugin file");
}
