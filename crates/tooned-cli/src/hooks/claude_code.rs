// SPDX-License-Identifier: AGPL-3.0-only

//! Claude Code hook integration: `tooned hook run --claude-code`,
//! `hook install --claude-code`.
//! See `specs/001-adaptive-toon-conversion/contracts/claude-code-hook.md`.

use std::path::PathBuf;

use super::{InstallError, Scope};

/// Runs the `PostToolUse` hook against stdin, printing
/// `hookSpecificOutput.updatedToolOutput` on a convert decision or nothing
/// on passthrough. Never panics and never itself decides the process exit
/// code -- the caller (`hooks::run`) always exits 0 for `hook run`, per
/// Claude Code's own platform-level fail-open guarantee plus tooned's own
/// independent no-panic guarantee (constitution Principle I).
pub fn run_hook() {
    super::run_hook_protocol(super::HookProtocol::ClaudeCode);
}

/// Default scope when `--scope` is not passed. Documented explicitly here
/// (plan.md left the default as a task-level decision rather than assuming
/// one silently): `Project` is the least-invasive choice, since it never
/// touches a developer's global Claude Code settings without being asked.
const DEFAULT_SCOPE: Scope = Scope::Project;

fn home_dir() -> Option<PathBuf> {
    for var in ["HOME", "USERPROFILE"] {
        if let Some(v) = std::env::var_os(var)
            && !v.is_empty()
        {
            return Some(PathBuf::from(v));
        }
    }
    None
}

pub(crate) fn settings_path(scope: Scope) -> Result<PathBuf, InstallError> {
    match scope {
        Scope::User => {
            let home = home_dir().ok_or(InstallError::NoHomeDirectory)?;
            Ok(home.join(".claude").join("settings.json"))
        }
        Scope::Project => {
            let cwd = std::env::current_dir().map_err(InstallError::CurrentDir)?;
            Ok(cwd.join(".claude").join("settings.json"))
        }
    }
}

/// Idempotently installs the `PostToolUse` hook entry into Claude Code's
/// `settings.json` (FR-016/FR-017): verifies `tooned` resolves on `PATH`
/// first, then merges by exact `command` string, never touching/reordering
/// any other entry in `hooks.PostToolUse`.
///
/// `mcp` is currently unused for the Claude Code target: this task's scope
/// (`contracts/claude-code-hook.md`) documents no Claude Code-specific MCP
/// registration surface (unlike Codex's `.mcp.json` bundling); accepted here
/// so the CLI surface stays uniform across `--claude-code`/`--codex`.
pub fn install(scope: Option<Scope>, _mcp: bool) -> Result<(), InstallError> {
    let Some(binary) = super::resolve_tooned_on_path() else {
        return Err(InstallError::BinaryNotOnPath);
    };
    let scope = match scope {
        Some(s) => s,
        None => DEFAULT_SCOPE,
    };
    let path = settings_path(scope)?;
    let command = super::hook_command_for(&binary, "claude-code");

    let mut root = super::read_json_value(&path)?;
    super::merge_post_tool_use_entry(&mut root, super::CLAUDE_CODE_MATCHER, &command);
    super::write_json_pretty(&path, &root)
}

/// Removes only tooned's own `PostToolUse` entry (matched by its command
/// suffix, `super::CLAUDE_CODE_COMMAND_SUFFIX` -- FR-018); a foreign entry,
/// or any other array element, is left byte-for-byte untouched. Returns
/// `true` if an entry was removed, `false` for "nothing to remove" (not
/// installed, or no settings file at all -- both are a graceful no-op, never
/// an error).
pub fn uninstall(scope: Option<Scope>) -> Result<bool, InstallError> {
    let scope = match scope {
        Some(s) => s,
        None => DEFAULT_SCOPE,
    };
    let path = settings_path(scope)?;
    let mut root = super::read_json_value(&path)?;
    let removed =
        super::remove_post_tool_use_entries_by_suffix(&mut root, super::CLAUDE_CODE_COMMAND_SUFFIX);
    if removed {
        super::write_json_pretty(&path, &root)?;
    }
    Ok(removed)
}

/// Read-only: is tooned's Claude Code hook currently installed? Checks both
/// scopes (`hook status` takes no `--scope` flag per `contracts/cli.md`), so
/// an install under either `--scope user` or `--scope project` is reported.
/// Never errors -- an unreadable/missing settings file, or an unresolvable
/// `--scope user` home directory, is simply "not installed" for that scope.
pub fn status() -> bool {
    [Scope::User, Scope::Project].into_iter().any(|scope| {
        let Ok(path) = settings_path(scope) else {
            return false;
        };
        let root = super::read_json_value(&path).unwrap_or_else(|_| serde_json::json!({}));
        super::has_post_tool_use_entry_by_suffix(&root, super::CLAUDE_CODE_COMMAND_SUFFIX)
    })
}
