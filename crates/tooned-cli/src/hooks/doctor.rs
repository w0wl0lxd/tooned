// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned hook doctor`: read-only report across all agents' configs,
//! listing every detected hook entry -- tooned's own and any foreign one
//! (e.g. rtk's) -- by `command`/`matcher`. Never writes to any config
//! file (data-model.md: "`tooned hook doctor` reads (never writes)...").

use std::fmt::Write as _;

use super::Scope;

const NOT_CONFIGURED: &str = "(not configured)";
const DASH: &str = "-";

/// Prints a report to stdout covering Claude Code (both `user` and
/// `project` scope), Codex (the current directory's `.codex-plugin/`
/// bundle), Devin CLI, Droid, OpenCode, Kilo, and Pi. Every step that
/// fails to read a config file is reported as an empty entry list for that
/// location rather than erroring -- `doctor` is a best-effort diagnostic,
/// never a hard failure.
pub fn run(json: bool) {
    let report = serde_json::json!({
        "claude_code": {
            "user": entries_report(super::claude_code::settings_path(Scope::User).ok(), super::CLAUDE_CODE_COMMAND_SUFFIX),
            "project": entries_report(super::claude_code::settings_path(Scope::Project).ok(), super::CLAUDE_CODE_COMMAND_SUFFIX),
        },
        "codex": {
            "project": entries_report(super::codex::hooks_json_path().ok(), super::CODEX_COMMAND_SUFFIX),
        },
        "devin": {
            "user": devin_entries_report(super::devin::settings_path(Scope::User).ok(), super::DEVIN_COMMAND_SUFFIX, true),
            "project": devin_entries_report(super::devin::settings_path(Scope::Project).ok(), super::DEVIN_COMMAND_SUFFIX, false),
        },
        "droid": {
            "user": entries_report(super::droid::settings_path(Scope::User).ok(), super::DROID_COMMAND_SUFFIX),
            "project": entries_report(super::droid::settings_path(Scope::Project).ok(), super::DROID_COMMAND_SUFFIX),
        },
        "opencode": super::opencode::doctor_report(),
        "kilo": super::kilo::doctor_report(),
        "pi": super::pi::doctor_report(),
    });

    if json {
        match sonic_rs::to_string_pretty(&report) {
            Ok(text) => println!("{text}"),
            Err(_) => println!("{report}"),
        }
        return;
    }

    print_human_report(&report);
}

fn print_human_report(report: &serde_json::Value) {
    let Some(agents) = report.as_object() else {
        return;
    };
    for (agent_name, scopes) in agents {
        println!("{agent_name}");
        let Some(scopes) = scopes.as_object() else {
            continue;
        };
        for (scope_name, scope_report) in scopes {
            let (path, status_note) = scope_summary(scope_report);
            let path_or_unknown = path.map_or(NOT_CONFIGURED, |p| p);
            println!("  {scope_name}: {path_or_unknown}{status_note}");

            if let Some(entries) = scope_report.get("entries").and_then(serde_json::Value::as_array)
            {
                for entry in entries {
                    let matcher = entry
                        .get("matcher")
                        .and_then(serde_json::Value::as_str)
                        .map_or(DASH, |v| v);
                    let command = entry
                        .get("command")
                        .and_then(serde_json::Value::as_str)
                        .map_or(DASH, |v| v);
                    let is_tooned =
                        entry.get("is_tooned").and_then(serde_json::Value::as_bool) == Some(true);
                    let marker = if is_tooned { " tooned" } else { "" };
                    println!("    {matcher:30} {command}{marker}");
                }
            } else if let Some(command) =
                scope_report.get("command").and_then(serde_json::Value::as_str)
                && !command.is_empty()
            {
                println!("    command: {command}");
            }

            if let Some(error) = scope_report.get("error").and_then(serde_json::Value::as_str) {
                println!("    error: {error}");
            }
        }
    }
}

fn scope_summary(scope_report: &serde_json::Value) -> (Option<&str>, String) {
    let path = scope_report.get("path").and_then(serde_json::Value::as_str);
    let mut note = String::new();
    if let Some(installed) = scope_report.get("installed").and_then(serde_json::Value::as_bool) {
        if installed {
            note.push_str(" [installed]");
        } else {
            note.push_str(" [not installed]");
        }
    } else if let Some(entries) = scope_report.get("entries").and_then(serde_json::Value::as_array)
    {
        let tooned_count = entries
            .iter()
            .filter(|e| e.get("is_tooned").and_then(serde_json::Value::as_bool) == Some(true))
            .count();
        let total = entries.len();
        if tooned_count > 0 {
            let _ = write!(note, " [{tooned_count}/{total} tooned entries]");
        } else if total > 0 {
            let _ = write!(note, " [{total} entries]");
        }
    }
    (path, note)
}

fn entries_report(path: Option<std::path::PathBuf>, own_suffix: &str) -> serde_json::Value {
    let Some(path) = path else {
        return serde_json::json!({ "path": null, "entries": [] });
    };
    let root = match super::read_json_value(&path) {
        Ok(value) => value,
        Err(e) => {
            return serde_json::json!({
                "path": path.display().to_string(),
                "entries": [],
                "error": e.to_string(),
            });
        }
    };
    let entries: Vec<serde_json::Value> = super::collect_post_tool_use_entries(&root)
        .into_iter()
        .map(|(matcher, command)| {
            serde_json::json!({
                "matcher": matcher,
                "command": command,
                "is_tooned": command.ends_with(own_suffix),
            })
        })
        .collect();
    serde_json::json!({ "path": path.display().to_string(), "entries": entries })
}

fn devin_entries_report(
    path: Option<std::path::PathBuf>,
    own_suffix: &str,
    nested: bool,
) -> serde_json::Value {
    let Some(path) = path else {
        return serde_json::json!({ "path": null, "entries": [] });
    };
    let root = match super::read_json_value(&path) {
        Ok(value) => value,
        Err(e) => {
            return serde_json::json!({
                "path": path.display().to_string(),
                "entries": [],
                "error": e.to_string(),
            });
        }
    };
    let entries: Vec<serde_json::Value> = super::devin::collect_entries(&root, nested)
        .into_iter()
        .map(|(matcher, command)| {
            serde_json::json!({
                "matcher": matcher,
                "command": command,
                "is_tooned": command.ends_with(own_suffix),
            })
        })
        .collect();
    serde_json::json!({ "path": path.display().to_string(), "entries": entries })
}
