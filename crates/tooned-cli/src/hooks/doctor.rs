// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned hook doctor`: read-only report across both agents' configs,
//! listing every detected hook entry -- tooned's own and any foreign one
//! (e.g. rtk's) -- by `command`/`matcher`. Never writes to either config
//! file (data-model.md: "`tooned hook doctor` reads (never writes)...").

use super::Scope;

/// Prints a JSON report to stdout covering Claude Code (both `user` and
/// `project` scope) and Codex (the current directory's `.codex-plugin/`
/// bundle). Every step that fails to read a config file (missing, unreadable,
/// malformed, or an unresolvable `--scope user` home directory) is reported
/// as an empty entry list for that location rather than erroring -- `doctor`
/// is a best-effort diagnostic, never a hard failure.
pub fn run() {
    let report = serde_json::json!({
        "claude_code": {
            "user": entries_report(super::claude_code::settings_path(Scope::User).ok(), super::CLAUDE_CODE_COMMAND_SUFFIX),
            "project": entries_report(super::claude_code::settings_path(Scope::Project).ok(), super::CLAUDE_CODE_COMMAND_SUFFIX),
        },
        "codex": {
            "project": entries_report(super::codex::hooks_json_path().ok(), super::CODEX_COMMAND_SUFFIX),
        },
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
