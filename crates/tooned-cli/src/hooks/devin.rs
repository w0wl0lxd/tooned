// SPDX-License-Identifier: AGPL-3.0-only

//! Devin CLI hook integration: `tooned hook run --devin`,
//! `hook install --devin`.
//!
//! Devin CLI reads hooks from `.devin/hooks.v1.json` (project scope) or
//! `~/.config/devin/config.json` (user scope). The project file uses the
//! event name as the top-level key (`{ "PostToolUse": [...] }`); the user
//! config nests hooks under a `"hooks"` key like Claude Code's settings.json.
//! See <https://docs.devin.ai/cli/extensibility/hooks/overview>.

use std::io::Read as _;
use std::path::PathBuf;

use super::{InstallError, Scope};

/// Default scope when `--scope` is not passed: project-local, because it
/// never touches a developer's global Devin settings without being asked.
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

fn project_hooks_path() -> Result<PathBuf, InstallError> {
    let cwd = std::env::current_dir().map_err(InstallError::CurrentDir)?;
    Ok(cwd.join(".devin").join("hooks.v1.json"))
}

fn user_config_path() -> Result<PathBuf, InstallError> {
    let home = home_dir().ok_or(InstallError::NoHomeDirectory)?;
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA").ok_or(InstallError::NoHomeDirectory)?;
        Ok(PathBuf::from(appdata).join("devin").join("config.json"))
    }
    #[cfg(not(windows))]
    {
        Ok(home.join(".config").join("devin").join("config.json"))
    }
}

pub(crate) fn settings_path(scope: Scope) -> Result<PathBuf, InstallError> {
    match scope {
        Scope::User => user_config_path(),
        Scope::Project => project_hooks_path(),
    }
}

fn is_nested_config(scope: Scope) -> bool {
    matches!(scope, Scope::User)
}

fn post_tool_use_array(
    root: &mut serde_json::Value,
    nested: bool,
) -> Option<&mut Vec<serde_json::Value>> {
    if nested {
        let hooks = root.as_object_mut()?.entry("hooks").or_insert_with(|| serde_json::json!({}));
        let arr =
            hooks.as_object_mut()?.entry("PostToolUse").or_insert_with(|| serde_json::json!([]));
        arr.as_array_mut()
    } else {
        let arr =
            root.as_object_mut()?.entry("PostToolUse").or_insert_with(|| serde_json::json!([]));
        arr.as_array_mut()
    }
}

fn merge_devin_entry(
    root: &mut serde_json::Value,
    matcher: &str,
    command: &str,
    nested: bool,
) -> bool {
    let Some(arr) = post_tool_use_array(root, nested) else {
        return false;
    };
    if arr.iter().any(|entry| super::entry_has_command(entry, command)) {
        return false;
    }
    arr.push(serde_json::json!({
        "matcher": matcher,
        "hooks": [ { "type": "command", "command": command } ],
    }));
    true
}

fn remove_devin_entries(root: &mut serde_json::Value, suffix: &str, nested: bool) -> bool {
    let Some(arr) = post_tool_use_array(root, nested) else {
        return false;
    };
    let before = arr.len();
    arr.retain(|entry| !super::entry_command_ends_with(entry, suffix));
    arr.len() != before
}

fn has_devin_entry(root: &serde_json::Value, suffix: &str, nested: bool) -> bool {
    let arr = if nested {
        root.get("hooks").and_then(|h| h.get("PostToolUse")).and_then(serde_json::Value::as_array)
    } else {
        root.get("PostToolUse").and_then(serde_json::Value::as_array)
    };
    arr.is_some_and(|a| a.iter().any(|entry| super::entry_command_ends_with(entry, suffix)))
}

/// Runs the `PostToolUse` hook against stdin, printing
/// `hookSpecificOutput.additionalContext` on a convert decision or nothing on
/// passthrough. Never panics and never itself decides the process exit code
/// -- the caller (`hooks::run`) always exits 0 for `hook run`, matching the
/// fail-safe behavior expected by Devin CLI command hooks.
pub fn run_hook() {
    let mut buf = Vec::new();
    let read_result = std::io::stdin().take(super::MAX_HOOK_STDIN_BYTES).read_to_end(&mut buf);
    if read_result.is_err() {
        return;
    }

    let outcome =
        std::panic::catch_unwind(|| super::process_hook_stdin(&buf, super::HookProtocol::Devin));
    if let Ok(Some(output)) = outcome {
        println!("{output}");
    }
}

/// Idempotently installs the `PostToolUse` hook entry into the Devin hooks
/// file for the requested scope. Verifies `tooned` resolves on `PATH` first,
/// then merges by exact `command` string, leaving every other entry untouched.
pub fn install(scope: Option<Scope>, _mcp: bool) -> Result<(), InstallError> {
    let Some(binary) = super::resolve_tooned_on_path() else {
        return Err(InstallError::BinaryNotOnPath);
    };
    let scope = match scope {
        Some(s) => s,
        None => DEFAULT_SCOPE,
    };
    let path = settings_path(scope)?;
    let command = format!("{} hook run --devin", binary.display());

    let mut root = super::read_json_value(&path);
    merge_devin_entry(&mut root, super::DEVIN_MATCHER, &command, is_nested_config(scope));
    super::write_json_pretty(&path, &root)
}

/// Removes only tooned's own `PostToolUse` entry (matched by its command
/// suffix, `super::DEVIN_COMMAND_SUFFIX`); a foreign entry is left
/// byte-for-byte untouched. Returns `true` if an entry was removed.
pub fn uninstall(scope: Option<Scope>) -> Result<bool, InstallError> {
    let scope = match scope {
        Some(s) => s,
        None => DEFAULT_SCOPE,
    };
    let path = settings_path(scope)?;
    let mut root = super::read_json_value(&path);
    let removed =
        remove_devin_entries(&mut root, super::DEVIN_COMMAND_SUFFIX, is_nested_config(scope));
    if removed {
        super::write_json_pretty(&path, &root)?;
    }
    Ok(removed)
}

/// Read-only: collect every `(matcher, command)` pair from Devin's
/// `PostToolUse` array, used by `hook doctor`.
pub fn collect_entries(root: &serde_json::Value, nested: bool) -> Vec<(String, String)> {
    let arr = if nested {
        root.get("hooks").and_then(|h| h.get("PostToolUse")).and_then(serde_json::Value::as_array)
    } else {
        root.get("PostToolUse").and_then(serde_json::Value::as_array)
    };
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in arr {
        let matcher = match entry.get("matcher").and_then(serde_json::Value::as_str) {
            Some(m) => m.to_string(),
            None => "<no matcher>".to_string(),
        };
        let Some(inner) = entry.get("hooks").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for h in inner {
            let command = match h.get("command").and_then(serde_json::Value::as_str) {
                Some(c) => c.to_string(),
                None => "<no command>".to_string(),
            };
            out.push((matcher.clone(), command));
        }
    }
    out
}

/// Read-only: is tooned's Devin hook currently installed? Checks both scopes
/// (`hook status` takes no `--scope` flag), so an install under either scope
/// is reported.
pub fn status() -> bool {
    [Scope::User, Scope::Project].into_iter().any(|scope| {
        let Ok(path) = settings_path(scope) else {
            return false;
        };
        let root = super::read_json_value(&path);
        has_devin_entry(&root, super::DEVIN_COMMAND_SUFFIX, is_nested_config(scope))
    })
}
