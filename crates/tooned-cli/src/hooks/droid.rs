// SPDX-License-Identifier: AGPL-3.0-only

//! Droid (Factory AI) hook integration: `tooned hook run --droid`,
//! `hook install --droid`.
//!
//! Droid reads shell command hooks from `.factory/hooks.json` (project scope)
//! or `~/.factory/hooks.json` (user scope). The config shape matches Claude
//! Code's `hooks.PostToolUse` array, but the `PostToolUse` stdin payload uses
//! `tool_response` as an object whose schema is tool-specific, so Droid's run
//! path does best-effort extraction. See <https://docs.factory.ai/reference/hooks-reference>.

use std::path::PathBuf;

use super::{InstallError, Scope};

/// Default scope when `--scope` is not passed: project-local, because it
/// never touches a developer's global Droid settings without being asked.
const DEFAULT_SCOPE: Scope = Scope::Project;

/// Droid `timeout` in seconds for the generated hook command. Chosen well
/// under any plausible Droid-side default while still generous for the
/// sub-5 ms conversion hot path plus a large stdin read.
const HOOK_TIMEOUT_SECONDS: u64 = 5;

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
            Ok(home.join(".factory").join("hooks.json"))
        }
        Scope::Project => {
            let cwd = std::env::current_dir().map_err(InstallError::CurrentDir)?;
            Ok(cwd.join(".factory").join("hooks.json"))
        }
    }
}

/// Runs the `PostToolUse` hook against stdin. Droid only supports
/// `additionalContext` in `PostToolUse`, which would append the TOON to the
/// original JSON rather than replace it, so this hook passthroughs on a
/// convert decision and prints nothing. Use `tooned wrap -- <cmd>` or
/// `... | tooned pipe` when TOON-only output is required. Never panics and
/// never itself decides the process exit code -- the caller (`hooks::run`)
/// always exits 0 for `hook run`, per Droid's command-hook fail-open expectation.
pub fn run_hook() {
    super::run_hook_protocol(super::HookProtocol::Droid);
}

fn merge_droid_entry(root: &mut serde_json::Value, matcher: &str, command: &str) -> bool {
    if !super::merge_post_tool_use_entry(root, matcher, command) {
        return false;
    }
    // `merge_post_tool_use_entry` only appends `{ type, command }`; Droid
    // additionally requires a per-command `timeout` so a stalled hook does
    // not block the agent. Walk the freshly appended entry and add it.
    if let Some(arr) = root
        .get_mut("hooks")
        .and_then(|h| h.get_mut("PostToolUse"))
        .and_then(serde_json::Value::as_array_mut)
        && let Some(last) = arr.last_mut()
        && let Some(hooks) = last.get_mut("hooks").and_then(serde_json::Value::as_array_mut)
        && let Some(first) = hooks.first_mut()
        && let Some(obj) = first.as_object_mut()
    {
        obj.insert("timeout".to_string(), serde_json::json!(HOOK_TIMEOUT_SECONDS));
    }
    true
}

/// Idempotently installs the `PostToolUse` hook entry into Droid's
/// `hooks.json` for the requested scope. Verifies `tooned` resolves on `PATH`
/// first, then merges by exact `command` string, leaving every other entry
/// untouched. The generated entry includes a 5-second `timeout` so Droid kills
/// a hung hook before any agent-side default would fire.
pub fn install(scope: Option<Scope>, _mcp: bool) -> Result<(), InstallError> {
    let Some(binary) = super::resolve_tooned_on_path() else {
        return Err(InstallError::BinaryNotOnPath);
    };
    let scope = match scope {
        Some(s) => s,
        None => DEFAULT_SCOPE,
    };
    let path = settings_path(scope)?;
    let command = super::hook_command_for(&binary, "droid");

    let mut root = super::read_json_value(&path)?;
    merge_droid_entry(&mut root, super::DROID_MATCHER, &command);
    super::write_json_pretty(&path, &root)
}

/// Removes only tooned's own `PostToolUse` entry (matched by its command
/// suffix, `super::DROID_COMMAND_SUFFIX`); a foreign entry is left
/// byte-for-byte untouched. Returns `true` if an entry was removed.
pub fn uninstall(scope: Option<Scope>) -> Result<bool, InstallError> {
    let scope = match scope {
        Some(s) => s,
        None => DEFAULT_SCOPE,
    };
    let path = settings_path(scope)?;
    let mut root = super::read_json_value(&path)?;
    let removed =
        super::remove_post_tool_use_entries_by_suffix(&mut root, super::DROID_COMMAND_SUFFIX);
    if removed {
        super::write_json_pretty(&path, &root)?;
    }
    Ok(removed)
}

/// Read-only: is tooned's Droid hook currently installed? Checks both scopes
/// (`hook status` takes no `--scope` flag), so an install under either scope
/// is reported.
pub fn status() -> bool {
    [Scope::User, Scope::Project].into_iter().any(|scope| {
        let Ok(path) = settings_path(scope) else {
            return false;
        };
        let root = super::read_json_value(&path).unwrap_or_else(|_| serde_json::json!({}));
        super::has_post_tool_use_entry_by_suffix(&root, super::DROID_COMMAND_SUFFIX)
    })
}
