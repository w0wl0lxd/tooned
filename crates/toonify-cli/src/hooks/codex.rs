//! Codex CLI hook integration: `tooned hook run --codex`,
//! `hook install --codex`.
//! See `specs/001-adaptive-toon-conversion/contracts/codex-hook.md`.

use std::io::Read as _;
use std::time::Duration;

use super::InstallError;

/// Internal watchdog bound for `hook run --codex`. Codex CLI does not
/// blanket-guarantee fail-open behavior for a hook process hang (unlike
/// Claude Code), so tooned's own binary MUST independently guarantee it
/// never blocks past a bounded timeout (constitution Principle I,
/// `contracts/codex-hook.md`). Chosen well under any plausible default
/// Codex-side hook timeout while still generous for the sub-5ms hot path
/// (constitution Technology Constraints).
const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(3);

/// Runs the `PostToolUse` hook against stdin with an internal watchdog:
/// the actual conversion work runs on a worker thread, and this function
/// returns (printing nothing further) as soon as either the worker finishes
/// or `WATCHDOG_TIMEOUT` elapses, whichever comes first. The caller
/// (`hooks::run`) always exits 0 for `hook run` immediately afterwards, so a
/// worker thread that is still running at that point is simply abandoned
/// (terminated by the process exit) rather than blocking the hook.
pub fn run_hook() {
    let mut buf = Vec::new();
    if std::io::stdin().read_to_end(&mut buf).is_err() {
        return;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let _worker = std::thread::spawn(move || {
        // Test-only stall injection so the watchdog bound itself can be
        // exercised end-to-end without depending on tooned-core (which is
        // deliberately fast, and therefore hard to stall legitimately) --
        // see `tests/codex_hook_watchdog.rs`. A normal hook invocation from
        // Codex CLI never sets this variable, so this is a no-op in
        // practice, not a behavior change to the real conversion path.
        if let Ok(raw) = std::env::var("TOONED_CODEX_TEST_STALL_MS")
            && let Ok(ms) = raw.parse::<u64>()
        {
            std::thread::sleep(Duration::from_millis(ms));
        }

        // Defense-in-depth: `process_hook_stdin` is designed to never
        // panic, but this hook sits directly in an agent's tool-call path,
        // so a slip in that guarantee must still fail safe.
        let outcome = std::panic::catch_unwind(|| {
            super::process_hook_stdin(&buf, super::HookProtocol::Codex)
        });
        let _ = tx.send(outcome.ok().flatten());
    });

    if let Ok(Some(output)) = rx.recv_timeout(WATCHDOG_TIMEOUT) {
        println!("{output}");
    }
}

/// Idempotently writes the `.codex-plugin/` bundle at the current directory
/// (FR-016/FR-017): verifies `tooned` resolves on `PATH` first, then merges
/// the `PostToolUse` hook entry by exact `command` string into
/// `.codex-plugin/hooks/hooks.json`, writes `.codex-plugin/plugin.json`
/// bundling it, and additionally writes `.codex-plugin/.mcp.json` (and
/// registers it in `plugin.json`) when `mcp` is set.
pub fn install(mcp: bool) -> Result<(), InstallError> {
    let Some(binary) = super::resolve_tooned_on_path() else {
        return Err(InstallError::BinaryNotOnPath);
    };
    let cwd = std::env::current_dir().map_err(InstallError::CurrentDir)?;
    let plugin_dir = cwd.join(".codex-plugin");
    let hooks_json_path = plugin_dir.join("hooks").join("hooks.json");
    let plugin_json_path = plugin_dir.join("plugin.json");
    let mcp_json_path = plugin_dir.join(".mcp.json");

    let command = format!("{} hook run --codex", binary.display());

    let mut hooks_root = super::read_json_value(&hooks_json_path);
    super::merge_post_tool_use_entry(&mut hooks_root, super::CODEX_MATCHER, &command);
    super::write_json_pretty(&hooks_json_path, &hooks_root)?;

    let mut plugin_json = serde_json::json!({
        "name": "tooned",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "Adaptive TOON re-encoding for AI agent tool-call context",
        "hooks": "./hooks/hooks.json",
    });

    if mcp {
        let mcp_json = serde_json::json!({
            "mcpServers": {
                "tooned": {
                    "command": binary.display().to_string(),
                    "args": ["mcp", "serve"],
                }
            }
        });
        super::write_json_pretty(&mcp_json_path, &mcp_json)?;
        if let Some(obj) = plugin_json.as_object_mut() {
            obj.insert(
                "mcpServers".to_string(),
                serde_json::Value::String("./.mcp.json".to_string()),
            );
        }
    }

    super::write_json_pretty(&plugin_json_path, &plugin_json)
}

pub(crate) fn hooks_json_path() -> Result<std::path::PathBuf, InstallError> {
    let cwd = std::env::current_dir().map_err(InstallError::CurrentDir)?;
    Ok(cwd.join(".codex-plugin").join("hooks").join("hooks.json"))
}

/// Removes only tooned's own `PostToolUse` entry from
/// `.codex-plugin/hooks/hooks.json` (matched by its command suffix,
/// `super::CODEX_COMMAND_SUFFIX` -- FR-018); a foreign entry is left
/// byte-for-byte untouched. Returns `true` if an entry was removed, `false`
/// for "nothing to remove" (not installed, or no bundle at all -- both are a
/// graceful no-op, never an error).
pub fn uninstall() -> Result<bool, InstallError> {
    let path = hooks_json_path()?;
    let mut root = super::read_json_value(&path);
    let removed =
        super::remove_post_tool_use_entries_by_suffix(&mut root, super::CODEX_COMMAND_SUFFIX);
    if removed {
        super::write_json_pretty(&path, &root)?;
    }
    Ok(removed)
}

/// Read-only: is tooned's Codex hook currently installed in the current
/// directory's `.codex-plugin/hooks/hooks.json`? Never errors -- an
/// unreadable/missing bundle is simply "not installed".
pub fn status() -> bool {
    let Ok(path) = hooks_json_path() else {
        return false;
    };
    let root = super::read_json_value(&path);
    super::has_post_tool_use_entry_by_suffix(&root, super::CODEX_COMMAND_SUFFIX)
}
