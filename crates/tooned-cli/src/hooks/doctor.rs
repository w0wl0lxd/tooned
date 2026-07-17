// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned hook doctor`: read-only report across all agents' configs,
//! listing every detected hook entry -- tooned's own and any foreign one
//! (e.g. rtk's) -- by `command`/`matcher`. Never writes to any config
//! file (data-model.md: "`tooned hook doctor` reads (never writes)...").

use super::Scope;

/// Prints a JSON report to stdout covering Claude Code (both `user` and
/// `project` scope), Codex (the current directory's `.codex-plugin/`
/// bundle), and Devin CLI (project `.devin/hooks.v1.json` and user
/// `~/.config/devin/config.json`). Every step that fails to read a config
/// file is reported as an empty entry list for that location rather than
/// erroring -- `doctor` is a best-effort diagnostic, never a hard failure.
pub fn run() {
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

    // `serde_json::to_string_pretty` on a `Value` we just built from known
    // scalar/array/object pieces cannot fail; propagate as best-effort text
    // rather than panicking if it somehow did.
    match serde_json::to_string_pretty(&report) {
        Ok(text) => println!("{text}"),
        Err(_) => println!("{report}"),
    }
}

fn entries_report(path: Option<std::path::PathBuf>, own_suffix: &str) -> serde_json::Value {
    let Some(path) = path else {
        return serde_json::json!({ "path": null, "entries": [] });
    };
    let root = super::read_json_value(&path);
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
    let root = super::read_json_value(&path);
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
