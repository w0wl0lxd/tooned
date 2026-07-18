// SPDX-License-Identifier: AGPL-3.0-only

//! `tooned hook` subcommands: `run`, `install`, `uninstall`, `status`,
//! `doctor`, for Claude Code, Codex CLI, Devin CLI, Droid, OpenCode, Kilo, and Pi.
//! See `specs/001-adaptive-toon-conversion/contracts/{claude-code-hook,codex-hook,devin-hook,droid-hook,opencode-hook,kilo-hook,pi-hook}.md`.

pub mod claude_code;
pub mod codex;
pub mod devin;
pub mod doctor;
pub mod droid;
pub mod kilo;
pub mod opencode;
pub mod pi;
pub mod plugin;

use std::io::Read;
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Scope {
    User,
    Project,
}

/// Exactly one of `--claude-code` / `--codex` / `--devin` selects the target agent.
// CLI arg struct with clap-generated bool flags; not a state machine.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args)]
pub struct AgentSelector {
    #[arg(long = "claude-code")]
    pub claude_code: bool,

    #[arg(long = "codex")]
    pub codex: bool,

    #[arg(long = "devin")]
    pub devin: bool,

    #[arg(long = "droid")]
    pub droid: bool,

    #[arg(long = "opencode")]
    pub opencode: bool,

    #[arg(long = "kilo")]
    pub kilo: bool,

    #[arg(long = "pi")]
    pub pi: bool,
}

/// Agent selector that also supports `--all` for commands that can operate on
/// every supported agent at once (`install`, `uninstall`, `status`).
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Args)]
pub struct AgentSelectorWithAll {
    #[command(flatten)]
    pub inner: AgentSelector,

    /// Target every supported agent instead of a single one.
    #[arg(long, group = "agent")]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct HookArgs {
    #[command(subcommand)]
    pub command: HookCommand,
}

#[derive(Debug, Subcommand)]
pub enum HookCommand {
    /// Run the hook against a `PostToolUse` payload on stdin (invoked by the agent).
    Run {
        #[command(flatten)]
        agent: AgentSelector,
    },
    /// Idempotent installer; verifies the `tooned` binary resolves before writing.
    Install {
        #[command(flatten)]
        agent: AgentSelectorWithAll,

        #[arg(long, value_enum)]
        scope: Option<Scope>,

        #[arg(long)]
        mcp: bool,
    },
    /// Removes only tooned's own entries.
    Uninstall {
        #[command(flatten)]
        agent: AgentSelectorWithAll,

        #[arg(long, value_enum)]
        scope: Option<Scope>,
    },
    /// Reports whether tooned's hook is currently installed.
    Status {
        #[command(flatten)]
        agent: AgentSelectorWithAll,
    },
    /// Reports all detected hook installations (tooned's and others') for all agents.
    Doctor,
}

/// Which agent an [`AgentSelector`] resolved to; `None` when neither or both
/// flags were passed. The `--all` variant is only produced by
/// [`resolve_agent_with_all`] for commands that can operate on every agent.
enum Agent {
    All,
    ClaudeCode,
    Codex,
    Devin,
    Droid,
    OpenCode,
    Kilo,
    Pi,
}

fn resolve_agent(agent: &AgentSelector) -> Option<Agent> {
    match (
        agent.claude_code,
        agent.codex,
        agent.devin,
        agent.droid,
        agent.opencode,
        agent.kilo,
        agent.pi,
    ) {
        (true, false, false, false, false, false, false) => Some(Agent::ClaudeCode),
        (false, true, false, false, false, false, false) => Some(Agent::Codex),
        (false, false, true, false, false, false, false) => Some(Agent::Devin),
        (false, false, false, true, false, false, false) => Some(Agent::Droid),
        (false, false, false, false, true, false, false) => Some(Agent::OpenCode),
        (false, false, false, false, false, true, false) => Some(Agent::Kilo),
        (false, false, false, false, false, false, true) => Some(Agent::Pi),
        _ => None,
    }
}

fn resolve_agent_with_all(agent: &AgentSelectorWithAll) -> Option<Agent> {
    if agent.all {
        // `--all` must not be combined with a specific agent flag.
        let any_specific = agent.inner.claude_code
            || agent.inner.codex
            || agent.inner.devin
            || agent.inner.droid
            || agent.inner.opencode
            || agent.inner.kilo
            || agent.inner.pi;
        if any_specific {
            return None;
        }
        return Some(Agent::All);
    }
    resolve_agent(&agent.inner)
}

const ALL_AGENTS: [Agent; 7] = [
    Agent::ClaudeCode,
    Agent::Codex,
    Agent::Devin,
    Agent::Droid,
    Agent::OpenCode,
    Agent::Kilo,
    Agent::Pi,
];

fn agent_label(agent: &Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => "claude-code",
        Agent::Codex => "codex",
        Agent::Devin => "devin",
        Agent::Droid => "droid",
        Agent::OpenCode => "opencode",
        Agent::Kilo => "kilo",
        Agent::Pi => "pi",
        Agent::All => "all",
    }
}

fn agent_display_name(agent: &Agent) -> &'static str {
    match agent {
        Agent::ClaudeCode => "Claude Code hook",
        Agent::Codex => "Codex hook",
        Agent::Devin => "Devin hook",
        Agent::Droid => "Droid hook",
        Agent::OpenCode => "OpenCode plugin",
        Agent::Kilo => "Kilo plugin",
        Agent::Pi => "Pi extension",
        Agent::All => "all hooks",
    }
}

fn install_agent(agent: &Agent, scope: Option<Scope>, mcp: bool) -> Result<(), InstallError> {
    match agent {
        Agent::ClaudeCode => claude_code::install(scope, mcp),
        Agent::Codex => codex::install(mcp),
        Agent::Devin => devin::install(scope, mcp),
        Agent::Droid => droid::install(scope, mcp),
        Agent::OpenCode => opencode::install(scope, mcp),
        Agent::Kilo => kilo::install(scope, mcp),
        Agent::Pi => pi::install(scope, mcp),
        Agent::All => Err(InstallError::Io(std::io::Error::other(
            "internal error: Agent::All passed to single-agent install",
        ))),
    }
}

fn uninstall_agent(agent: &Agent, scope: Option<Scope>) -> Result<bool, InstallError> {
    match agent {
        Agent::ClaudeCode => claude_code::uninstall(scope),
        Agent::Codex => codex::uninstall(),
        Agent::Devin => devin::uninstall(scope),
        Agent::Droid => droid::uninstall(scope),
        Agent::OpenCode => opencode::uninstall(scope),
        Agent::Kilo => kilo::uninstall(scope),
        Agent::Pi => pi::uninstall(scope),
        Agent::All => Err(InstallError::Io(std::io::Error::other(
            "internal error: Agent::All passed to single-agent uninstall",
        ))),
    }
}

fn status_agent(agent: &Agent) -> bool {
    match agent {
        Agent::ClaudeCode => claude_code::status(),
        Agent::Codex => codex::status(),
        Agent::Devin => devin::status(),
        Agent::Droid => droid::status(),
        Agent::OpenCode => opencode::status(),
        Agent::Kilo => kilo::status(),
        Agent::Pi => pi::status(),
        Agent::All => false,
    }
}

/// Exit code used across `hook install`/`hook run` for conditions that
/// aren't a payload-driven passthrough decision (contracts/cli.md).
const EXIT_USAGE_ERROR: i32 = 2;
const EXIT_BINARY_NOT_ON_PATH: i32 = 4;

/// Matchers exactly as specified by the contracts (verified, not guessed --
/// see `specs/001-adaptive-toon-conversion/contracts/{claude-code-hook,codex-hook,devin-hook}.md`).
pub(crate) const CLAUDE_CODE_MATCHER: &str = "Bash|Read|Grep|WebFetch|^mcp__";
pub(crate) const CODEX_MATCHER: &str = "Bash";
pub(crate) const DEVIN_MATCHER: &str = "^exec$|^read$|^edit$|^grep$|^glob$|^mcp__";
pub(crate) const DROID_MATCHER: &str = "Execute|Read|Grep|Glob|FetchUrl|WebSearch|^mcp__";

/// Upper bound on how many raw stdin bytes a `hook run` invocation will ever
/// buffer, applied *before* any JSON parsing happens. This sits directly in
/// an agent's tool-call path (Claude Code/Codex CLI/Devin CLI serialize the
/// wrapped tool's entire result -- which can be multi-GB for e.g. `cat` on a
/// huge file, or a large `WebFetch`/`mcp__*` result -- straight onto this
/// process's stdin), so an unbounded `read_to_end` here can OOM or badly
/// stall the hook well before `ConversionOptions::default().max_input_bytes`
/// (2 MiB) is ever consulted downstream.
///
/// Deliberately larger than that 2 MiB cap (not equal to it): the raw stdin
/// payload also carries the JSON envelope (`hook_event_name`, `tool_name`,
/// `tool_input`, etc.) plus `\"`/`\\`-escaping overhead for whatever
/// `tool_output`/`tool_response` text it wraps, so a legitimately
/// convertible-sized tool result can be noticeably larger on the wire than
/// its decoded form. 8x the default cap plus a fixed envelope allowance
/// comfortably covers that overhead while still bounding a multi-GB input to
/// a fixed, small amount of memory.
pub(crate) const MAX_HOOK_STDIN_BYTES: u64 = 8 * 2 * 1024 * 1024 + 64 * 1024;

/// Errors that can occur while installing a hook. Never surfaces as a panic;
/// `hooks::run` maps this to a clear stderr message and the exit code
/// `contracts/cli.md` documents (4 for `BinaryNotOnPath`, 1 otherwise).
#[derive(Debug, thiserror::Error)]
pub(crate) enum InstallError {
    #[error(
        "could not resolve a `tooned` binary on PATH; install it first \
         (e.g. `cargo install tooned`, or a prebuilt release binary) so it is \
         discoverable on PATH before running `tooned hook install`"
    )]
    BinaryNotOnPath,
    #[error(
        "could not determine a user-level config directory for --scope user \
         (no $HOME/%USERPROFILE% or, on Windows, %APPDATA% is set)"
    )]
    NoHomeDirectory,
    #[error("failed to determine the current directory: {0}")]
    CurrentDir(#[source] std::io::Error),
    #[error("failed to read or write hook configuration: {0}")]
    Io(#[source] std::io::Error),
}

impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        InstallError::Io(e)
    }
}

/// Resolves the `tooned` binary strictly via a `PATH` search (never via
/// `std::env::current_exe()`), so an install invoked via a path outside
/// `PATH` still correctly fails when `tooned` is not separately available
/// on `PATH` (data-model.md: "Install MUST resolve the tooned binary on
/// PATH ... and abort with a clear error before writing any entry if that
/// resolution fails").
pub(crate) fn resolve_tooned_on_path() -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exe_name = if cfg!(windows) { "tooned.exe" } else { "tooned" };
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(exe_name);
        if is_executable_file(&candidate) { Some(candidate) } else { None }
    })
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Shell-quote the resolved `tooned` binary path so that a `PostToolUse`
/// command string remains a single executable even when the path contains
/// spaces or shell metacharacters. Falls back to single-quote escaping if
/// `shlex` refuses the path (e.g. contains a NUL byte, which cannot occur in
/// a valid filesystem path from `which`).
pub(crate) fn quote_binary_for_shell(binary: &Path) -> String {
    let lossy = binary.to_string_lossy();
    match shlex::try_quote(&lossy) {
        Ok(q) => q.into_owned(),
        Err(_) => format!("'{}'", lossy.replace('\'', "'\\''")),
    }
}

/// Build the agent-specific `PostToolUse` command string, with the `tooned`
/// binary path safely quoted for execution by a shell.
pub(crate) fn hook_command_for(binary: &Path, flag: &str) -> String {
    format!("{} hook run --{flag}", quote_binary_for_shell(binary))
}

/// Parses `path` as a JSON object. A missing file starts fresh (`{}`);
/// a malformed file is an error so the installer never silently overwrites
/// a user's existing agent configuration.
pub(crate) fn read_json_value(path: &Path) -> Result<serde_json::Value, InstallError> {
    match std::fs::read(path) {
        Ok(bytes) => sonic_rs::from_slice::<serde_json::Value>(&bytes).map_err(|e| {
            InstallError::Io(std::io::Error::other(format!(
                "{} is not valid JSON ({e}); fix or remove it before installing the hook",
                path.display()
            )))
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::json!({})),
        Err(e) => Err(InstallError::Io(e)),
    }
}

/// Writes `value` as pretty-printed JSON to `path`, creating parent
/// directories as needed.
///
/// Hardened against concurrent-writer corruption (T071, spec.md Edge Cases:
/// concurrent installer runs): the JSON is first written in full to a
/// uniquely-named temp file in the *same* directory as `path`, then promoted
/// into place with a single `rename` -- on all platforms tooned targets,
/// a rename within one directory is atomic, so a reader (or another
/// installer run) never observes a partially-written file, unlike a direct
/// in-place `fs::write`.
pub(crate) fn write_json_pretty(
    path: &Path,
    value: &serde_json::Value,
) -> Result<(), InstallError> {
    let parent = path.parent().ok_or_else(|| {
        InstallError::Io(std::io::Error::other("target path has no parent directory"))
    })?;
    std::fs::create_dir_all(parent)?;
    let text = sonic_rs::to_string_pretty(value).map_err(|e| {
        InstallError::Io(std::io::Error::other(format!("failed to serialize hook config: {e}")))
    })?;

    let file_name = path
        .file_name()
        .ok_or_else(|| InstallError::Io(std::io::Error::other("target path has no file name")))?
        .to_string_lossy();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp_name = format!(".{file_name}.tmp.{}.{nanos}", std::process::id());
    let tmp_path = parent.join(tmp_name);

    std::fs::write(&tmp_path, &text)?;
    let rename_result = std::fs::rename(&tmp_path, path);
    if rename_result.is_err() {
        // Best-effort cleanup; the rename error itself is what's surfaced.
        let _ = std::fs::remove_file(&tmp_path);
    }
    rename_result?;
    Ok(())
}

/// Idempotently ensures `root.hooks.PostToolUse` contains an entry whose
/// inner `hooks[].command` equals `command` -- appends a new
/// `{matcher, hooks: [{type: "command", command}]}` entry only if no
/// existing entry's command already matches (FR-016); every other array
/// element is left untouched (FR-017). Returns `true` if a new entry was
/// appended.
pub(crate) fn merge_post_tool_use_entry(
    root: &mut serde_json::Value,
    matcher: &str,
    command: &str,
) -> bool {
    if !root.is_object() {
        *root = serde_json::json!({});
    }
    let Some(root_obj) = root.as_object_mut() else {
        return false;
    };
    let hooks_val = root_obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
    if !hooks_val.is_object() {
        *hooks_val = serde_json::json!({});
    }
    let Some(hooks_obj) = hooks_val.as_object_mut() else {
        return false;
    };
    let arr_val = hooks_obj.entry("PostToolUse").or_insert_with(|| serde_json::json!([]));
    if !arr_val.is_array() {
        *arr_val = serde_json::json!([]);
    }
    let Some(arr) = arr_val.as_array_mut() else {
        return false;
    };

    for entry in arr.iter_mut() {
        let Some(hooks) = entry.get_mut("hooks").and_then(|h| h.as_array_mut()) else {
            continue;
        };
        for h in hooks.iter_mut() {
            let Some(existing) = h.get("command").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if existing == command {
                return false;
            }
            if let Some(suffix) = command_suffix_for(command)
                && existing.ends_with(suffix)
            {
                // Reinstall or PATH change: update the existing entry to the
                // new binary path in-place rather than leaving a stale path
                // behind (finding: duplicate PostToolUse entries on reinstall).
                h["command"] = serde_json::json!(command);
                return false;
            }
        }
    }

    arr.push(serde_json::json!({
        "matcher": matcher,
        "hooks": [ { "type": "command", "command": command } ],
    }));
    true
}

/// True if any inner `hooks[].command` of `entry` exactly matches `command`.
fn entry_has_command(entry: &serde_json::Value, command: &str) -> bool {
    entry.get("hooks").and_then(serde_json::Value::as_array).is_some_and(|inner| {
        inner.iter().any(|h| h.get("command").and_then(serde_json::Value::as_str) == Some(command))
    })
}

/// True if any inner `hooks[].command` of `entry` ends with `suffix`.
fn entry_command_ends_with(entry: &serde_json::Value, suffix: &str) -> bool {
    entry.get("hooks").and_then(serde_json::Value::as_array).is_some_and(|inner| {
        inner.iter().any(|h| {
            h.get("command")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|c| c.ends_with(suffix))
        })
    })
}

/// Returns the tooned-owned suffix `command` ends with, if any.
fn command_suffix_for(command: &str) -> Option<&'static str> {
    const SUFFIXES: &[&str] = &[
        CLAUDE_CODE_COMMAND_SUFFIX,
        CODEX_COMMAND_SUFFIX,
        DEVIN_COMMAND_SUFFIX,
        DROID_COMMAND_SUFFIX,
    ];
    SUFFIXES.iter().copied().find(|s| command.ends_with(*s))
}

/// Command suffixes that identify tooned's own `PostToolUse` entries,
/// independent of the absolute binary path prefix (which may legitimately
/// differ between an `install` and a later `uninstall`/`status` run, e.g.
/// after a reinstall to a new location) -- see data-model.md's "Integration
/// Installation Record" identity rules (FR-016/FR-018).
pub(crate) const CLAUDE_CODE_COMMAND_SUFFIX: &str = "hook run --claude-code";
pub(crate) const CODEX_COMMAND_SUFFIX: &str = "hook run --codex";
pub(crate) const DEVIN_COMMAND_SUFFIX: &str = "hook run --devin";
pub(crate) const DROID_COMMAND_SUFFIX: &str = "hook run --droid";

/// Removes every `hooks.PostToolUse` entry whose inner command ends with
/// `suffix` (FR-018); every other entry, including a foreign tool's, is left
/// untouched. Returns `true` if at least one entry was removed.
pub(crate) fn remove_post_tool_use_entries_by_suffix(
    root: &mut serde_json::Value,
    suffix: &str,
) -> bool {
    let Some(arr) = root
        .get_mut("hooks")
        .and_then(|h| h.get_mut("PostToolUse"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };
    let before = arr.len();
    arr.retain(|entry| !entry_command_ends_with(entry, suffix));
    arr.len() != before
}

/// Read-only check: does `root.hooks.PostToolUse` contain an entry whose
/// inner command ends with `suffix`?
pub(crate) fn has_post_tool_use_entry_by_suffix(root: &serde_json::Value, suffix: &str) -> bool {
    root.get("hooks")
        .and_then(|h| h.get("PostToolUse"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|arr| arr.iter().any(|entry| entry_command_ends_with(entry, suffix)))
}

/// Read-only: every `(matcher, command)` pair found in `root.hooks.PostToolUse`,
/// used by `hook doctor` to report both tooned's own and any foreign entries.
pub(crate) fn collect_post_tool_use_entries(root: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(arr) =
        root.get("hooks").and_then(|h| h.get("PostToolUse")).and_then(serde_json::Value::as_array)
    else {
        return out;
    };
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

/// Which agent's `PostToolUse` stdin/stdout shape [`process_hook_stdin`]
/// should speak. Claude Code and Codex CLI do NOT share a stdin field name
/// or an output field name for this hook -- verified directly against
/// `openai/codex`'s `codex-rs/hooks/src/events/post_tool_use.rs` and
/// `output_parser::parse_post_tool_use()` (see `contracts/codex-hook.md`'s
/// I/O contract section), which is why this is parameterized rather than
/// assuming one shared shape (an earlier, unverified assumption that both
/// hooks looked exactly like Claude Code's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookProtocol {
    ClaudeCode,
    Codex,
    Devin,
    Droid,
    OpenCode,
    Kilo,
    Pi,
}

impl HookProtocol {
    /// Extracts the raw tool-output bytes from a `PostToolUse` stdin payload.
    /// Claude Code uses `tool_output`; Codex uses `tool_response` as a raw
    /// string/object; Devin uses `tool_response.output` (a string inside an
    /// object that also carries `success`/`error`).
    fn extract_bytes(self, payload: &serde_json::Value) -> Option<Vec<u8>> {
        match self {
            HookProtocol::ClaudeCode
            | HookProtocol::OpenCode
            | HookProtocol::Kilo
            | HookProtocol::Pi => {
                let value = payload.get("tool_output")?;
                Some(match value {
                    serde_json::Value::String(s) => s.as_bytes().to_vec(),
                    other => sonic_rs::to_vec(other).ok()?,
                })
            }
            HookProtocol::Codex => {
                let value = payload.get("tool_response")?;
                Some(match value {
                    serde_json::Value::String(s) => s.as_bytes().to_vec(),
                    other => sonic_rs::to_vec(other).ok()?,
                })
            }
            HookProtocol::Devin => {
                let response = payload.get("tool_response")?;
                match response {
                    serde_json::Value::String(s) => Some(s.as_bytes().to_vec()),
                    serde_json::Value::Object(_) => {
                        let output = response.get("output")?;
                        match output {
                            serde_json::Value::String(s) => Some(s.as_bytes().to_vec()),
                            other => sonic_rs::to_vec(other).ok(),
                        }
                    }
                    other => sonic_rs::to_vec(other).ok(),
                }
            }
            HookProtocol::Droid => {
                let value = payload.get("tool_response")?;
                Some(match value {
                    serde_json::Value::String(s) => s.as_bytes().to_vec(),
                    serde_json::Value::Object(obj) => {
                        // Droid tool_response schemas are tool-specific. Try the
                        // common string-valued output fields first, then MCP-style
                        // content arrays, then arrays/objects that are themselves
                        // the payload to encode. Fall back to the full object JSON.
                        if let Some(s) = obj.get("output").and_then(serde_json::Value::as_str) {
                            s.as_bytes().to_vec()
                        } else if let Some(s) =
                            obj.get("content").and_then(serde_json::Value::as_str)
                        {
                            s.as_bytes().to_vec()
                        } else if let Some(s) =
                            obj.get("stdout").and_then(serde_json::Value::as_str)
                        {
                            s.as_bytes().to_vec()
                        } else if let Some(s) =
                            obj.get("result").and_then(serde_json::Value::as_str)
                        {
                            s.as_bytes().to_vec()
                        } else if let Some(s) = obj.get("text").and_then(serde_json::Value::as_str)
                        {
                            s.as_bytes().to_vec()
                        } else if let Some(arr) =
                            obj.get("content").and_then(serde_json::Value::as_array)
                        {
                            let mut text = String::new();
                            for item in arr {
                                if let Some(t) =
                                    item.get("text").and_then(serde_json::Value::as_str)
                                {
                                    if !text.is_empty() {
                                        text.push('\n');
                                    }
                                    text.push_str(t);
                                }
                            }
                            if text.is_empty() {
                                sonic_rs::to_vec(value).ok()?
                            } else {
                                text.into_bytes()
                            }
                        } else {
                            sonic_rs::to_vec(value).ok()?
                        }
                    }
                    other => sonic_rs::to_vec(other).ok()?,
                })
            }
        }
    }
}

/// Reads a `PostToolUse` stdin payload (per `contracts/claude-code-hook.md`,
/// `contracts/codex-hook.md`, and Devin CLI's hook docs), extracts the tool's
/// raw output, and runs it through [`tooned_core::maybe_tooned`]. Returns the
/// JSON string to print to stdout on a convert decision, or `None` for
/// passthrough (passthrough means "print nothing" per the contracts, not
/// echoing the original bytes back out -- the host platform already preserves
/// the original tool output whenever the hook prints nothing).
///
/// The emitted `hookSpecificOutput` shape depends on `protocol`: Claude Code
/// supports replacing the tool's output in place via `updatedToolOutput`;
/// Codex and Devin only recognize `additionalContext` for surfacing extra
/// content, so that's emitted for those protocols.
///
/// Never panics for any `stdin` byte slice, including invalid UTF-8 or
/// malformed/adversarial JSON -- every fallible step folds into `None`
/// rather than propagating an error or panicking (constitution Principle I).
/// Callers additionally wrap this in `std::panic::catch_unwind` as
/// defense-in-depth (see `claude_code::run_hook`/`codex::run_hook`/`devin::run_hook`).
pub(crate) fn process_hook_stdin(stdin: &[u8], protocol: HookProtocol) -> Option<String> {
    let payload: serde_json::Value = sonic_rs::from_slice::<serde_json::Value>(stdin).ok()?;
    let bytes = protocol.extract_bytes(&payload)?;

    let opts = tooned_core::ConversionOptions::default();
    let conversion = tooned_core::maybe_tooned(&bytes, &opts).ok()?;
    let result = match conversion {
        tooned_core::Conversion::Toon { text, .. } => {
            let out = match protocol {
                HookProtocol::ClaudeCode
                | HookProtocol::OpenCode
                | HookProtocol::Kilo
                | HookProtocol::Pi => serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PostToolUse",
                        "updatedToolOutput": text,
                    }
                }),
                HookProtocol::Codex | HookProtocol::Devin | HookProtocol::Droid => {
                    serde_json::json!({
                        "hookSpecificOutput": {
                            "hookEventName": "PostToolUse",
                            "additionalContext": text,
                        }
                    })
                }
            };
            sonic_rs::to_string(&out).ok()
        }
        tooned_core::Conversion::Passthrough { .. } => None,
    };
    {
        #[allow(clippy::single_match_else, clippy::manual_unwrap_or)]
        let (converted, output_len) = if let Some(s) = &result {
            let len = match s.len().try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            };
            (true, len)
        } else {
            let len = match bytes.len().try_into() {
                Ok(v) => v,
                Err(_) => i64::MAX,
            };
            (false, len)
        };
        let scope = match protocol {
            HookProtocol::ClaudeCode => crate::metrics_recorder::CliSurface::HookClaude,
            HookProtocol::Codex => crate::metrics_recorder::CliSurface::HookCodex,
            HookProtocol::Devin => crate::metrics_recorder::CliSurface::HookDevin,
            HookProtocol::Droid => crate::metrics_recorder::CliSurface::HookDroid,
            HookProtocol::OpenCode => crate::metrics_recorder::CliSurface::HookOpenCode,
            HookProtocol::Kilo => crate::metrics_recorder::CliSurface::HookKilo,
            HookProtocol::Pi => crate::metrics_recorder::CliSurface::HookPi,
        };
        #[allow(clippy::manual_unwrap_or)]
        let input_len = match bytes.len().try_into() {
            Ok(v) => v,
            Err(_) => i64::MAX,
        };
        crate::metrics_recorder::record_convert_outcome(
            scope,
            &crate::metrics_recorder::SourceLabel::None,
            None,
            converted,
            input_len,
            output_len,
        );
    }
    result
}

/// Reads stdin up to [`MAX_HOOK_STDIN_BYTES`], runs it through
/// [`process_hook_stdin`] with the given protocol, and prints any hook
/// decision to stdout. This is the shared runtime body for all agents whose
/// hook execution model is a simple synchronous command (Claude Code, Devin,
/// Droid, OpenCode, Kilo, Pi). Codex keeps its own watchdog path.
///
/// The caller (`hooks::run`) always exits 0 for `hook run`, so a failure here
/// simply prints nothing and lets the agent fall back to the original output.
pub(crate) fn run_hook_protocol(protocol: HookProtocol) {
    let mut buf = Vec::new();
    let read_result = std::io::stdin().take(MAX_HOOK_STDIN_BYTES).read_to_end(&mut buf);
    if read_result.is_err() {
        return;
    }

    let outcome = std::panic::catch_unwind(|| process_hook_stdin(&buf, protocol));
    if let Ok(Some(output)) = outcome {
        println!("{output}");
    }
}

fn install_exit_code(err: &InstallError) -> i32 {
    match err {
        InstallError::BinaryNotOnPath => EXIT_BINARY_NOT_ON_PATH,
        InstallError::NoHomeDirectory | InstallError::CurrentDir(_) | InstallError::Io(_) => 1,
    }
}

/// No branch here ever surfaces an `Err` value -- every failure path exits
/// the process directly (`std::process::exit`) with the specific code
/// `contracts/cli.md` documents, rather than propagating a `Result`, so this
/// deliberately returns `()` rather than `anyhow::Result<()>`.
pub fn run(args: &HookArgs) {
    match &args.command {
        HookCommand::Run { agent } => {
            match resolve_agent(agent) {
                Some(Agent::ClaudeCode) => claude_code::run_hook(),
                Some(Agent::Codex) => codex::run_hook(),
                Some(Agent::Devin) => devin::run_hook(),
                Some(Agent::Droid) => droid::run_hook(),
                Some(Agent::OpenCode) => opencode::run_hook(),
                Some(Agent::Kilo) => kilo::run_hook(),
                Some(Agent::Pi) => pi::run_hook(),
                // No/ambiguous agent selection on `hook run` is itself a
                // form of doubt -- the contract's fail-safe exit-0 guarantee
                // applies uniformly, not just to payload-driven failure.
                None | Some(Agent::All) => {}
            }
            // Contract: `hook run` ALWAYS exits 0, regardless of internal
            // outcome -- a non-zero exit is itself a form of "loud failure"
            // the fail-safe principle forbids.
            std::process::exit(0);
        }
        HookCommand::Install { agent, scope, mcp } => match resolve_agent_with_all(agent) {
            Some(Agent::ClaudeCode) => {
                if let Err(e) = claude_code::install(*scope, *mcp) {
                    eprintln!("tooned hook install --claude-code: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            }
            Some(Agent::Codex) => {
                // Codex has no `--scope user|project` concept (unlike Claude
                // Code): `codex::install` always writes the project-local
                // `.codex-plugin/` bundle. Warn rather than silently
                // discarding the flag, so a caller who passed `--scope`
                // expecting it to take effect (or to be rejected) isn't left
                // unaware the request was ignored (contracts/cli.md
                // documents `--scope` generically across both agents, with
                // no caveat that it's a no-op for `--codex`).
                if scope.is_some() {
                    eprintln!(
                        "tooned hook install --codex: --scope has no effect for Codex CLI \
                         (no user/project scope concept exists); the plugin is always written \
                         to ./.codex-plugin/ in the current directory"
                    );
                }
                if let Err(e) = codex::install(*mcp) {
                    eprintln!("tooned hook install --codex: {e}");
                    std::process::exit(install_exit_code(&e));
                }
                eprintln!(
                    "tooned: Codex CLI hook installed under .codex-plugin/. This is a \
                     non-managed hook, so Codex CLI requires an explicit trust review before \
                     it will fire: run `/hooks` inside Codex CLI now to review and trust it."
                );
            }
            Some(Agent::Devin) => {
                if let Err(e) = devin::install(*scope, *mcp) {
                    eprintln!("tooned hook install --devin: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            }
            Some(Agent::Droid) => {
                if let Err(e) = droid::install(*scope, *mcp) {
                    eprintln!("tooned hook install --droid: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            }
            Some(Agent::OpenCode) => {
                if let Err(e) = opencode::install(*scope, *mcp) {
                    eprintln!("tooned hook install --opencode: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            }
            Some(Agent::Kilo) => {
                if let Err(e) = kilo::install(*scope, *mcp) {
                    eprintln!("tooned hook install --kilo: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            }
            Some(Agent::Pi) => {
                if let Err(e) = pi::install(*scope, *mcp) {
                    eprintln!("tooned hook install --pi: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            }
            Some(Agent::All) => {
                let mut failed = false;
                let mut installed = Vec::new();
                for a in &ALL_AGENTS {
                    if matches!(a, Agent::Codex) && scope.is_some() {
                        eprintln!(
                            "tooned hook install --codex: --scope has no effect for Codex CLI \
                             (no user/project scope concept exists); the plugin is always written \
                             to ./.codex-plugin/ in the current directory"
                        );
                    }
                    if matches!(a, Agent::Codex) {
                        match codex::install(*mcp) {
                            Ok(()) => {
                                installed.push(agent_label(a));
                                eprintln!(
                                    "tooned: Codex CLI hook installed under .codex-plugin/. This is a \
                                     non-managed hook, so Codex CLI requires an explicit trust review before \
                                     it will fire: run `/hooks` inside Codex CLI now to review and trust it."
                                );
                            }
                            Err(e) => {
                                eprintln!("tooned hook install --codex: {e}");
                                failed = true;
                            }
                        }
                        continue;
                    }
                    match install_agent(a, *scope, *mcp) {
                        Ok(()) => installed.push(agent_label(a)),
                        Err(e) => {
                            eprintln!("tooned hook install --{}: {e}", agent_label(a));
                            failed = true;
                        }
                    }
                }
                if failed {
                    std::process::exit(1);
                }
                println!("tooned: installed hooks for {}", installed.join(", "));
            }
            None => {
                eprintln!(
                    "tooned hook install: specify exactly one of --all, --claude-code, --codex, --devin, --droid, --opencode, --kilo, or --pi"
                );
                std::process::exit(EXIT_USAGE_ERROR);
            }
        },
        HookCommand::Uninstall { agent, scope } => match resolve_agent_with_all(agent) {
            Some(Agent::ClaudeCode) => match claude_code::uninstall(*scope) {
                Ok(true) => println!("tooned: removed the Claude Code hook entry"),
                Ok(false) => {
                    println!("tooned: nothing to remove (Claude Code hook not installed)");
                }
                Err(e) => {
                    eprintln!("tooned hook uninstall --claude-code: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            },
            Some(Agent::Codex) => {
                if scope.is_some() {
                    eprintln!(
                        "tooned hook uninstall --codex: --scope has no effect for Codex CLI \
                         (no user/project scope concept exists); only ./.codex-plugin/ in the \
                         current directory is ever touched"
                    );
                }
                match codex::uninstall() {
                    Ok(true) => println!("tooned: removed the Codex hook entry"),
                    Ok(false) => {
                        println!("tooned: nothing to remove (Codex hook not installed)");
                    }
                    Err(e) => {
                        eprintln!("tooned hook uninstall --codex: {e}");
                        std::process::exit(install_exit_code(&e));
                    }
                }
            }
            Some(Agent::Devin) => match devin::uninstall(*scope) {
                Ok(true) => println!("tooned: removed the Devin hook entry"),
                Ok(false) => {
                    println!("tooned: nothing to remove (Devin hook not installed)");
                }
                Err(e) => {
                    eprintln!("tooned hook uninstall --devin: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            },
            Some(Agent::Droid) => match droid::uninstall(*scope) {
                Ok(true) => println!("tooned: removed the Droid hook entry"),
                Ok(false) => {
                    println!("tooned: nothing to remove (Droid hook not installed)");
                }
                Err(e) => {
                    eprintln!("tooned hook uninstall --droid: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            },
            Some(Agent::OpenCode) => match opencode::uninstall(*scope) {
                Ok(true) => println!("tooned: removed the OpenCode plugin"),
                Ok(false) => {
                    println!("tooned: nothing to remove (OpenCode plugin not installed)");
                }
                Err(e) => {
                    eprintln!("tooned hook uninstall --opencode: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            },
            Some(Agent::Kilo) => match kilo::uninstall(*scope) {
                Ok(true) => println!("tooned: removed the Kilo plugin"),
                Ok(false) => {
                    println!("tooned: nothing to remove (Kilo plugin not installed)");
                }
                Err(e) => {
                    eprintln!("tooned hook uninstall --kilo: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            },
            Some(Agent::Pi) => match pi::uninstall(*scope) {
                Ok(true) => println!("tooned: removed the Pi extension"),
                Ok(false) => {
                    println!("tooned: nothing to remove (Pi extension not installed)");
                }
                Err(e) => {
                    eprintln!("tooned hook uninstall --pi: {e}");
                    std::process::exit(install_exit_code(&e));
                }
            },
            Some(Agent::All) => {
                let mut failed = false;
                let mut removed = Vec::new();
                for a in &ALL_AGENTS {
                    if matches!(a, Agent::Codex) && scope.is_some() {
                        eprintln!(
                            "tooned hook uninstall --codex: --scope has no effect for Codex CLI \
                             (no user/project scope concept exists); only ./.codex-plugin/ in the \
                             current directory is ever touched"
                        );
                    }
                    match uninstall_agent(a, *scope) {
                        Ok(true) => removed.push(agent_label(a)),
                        Ok(false) => {}
                        Err(e) => {
                            eprintln!("tooned hook uninstall --{}: {e}", agent_label(a));
                            failed = true;
                        }
                    }
                }
                if failed {
                    std::process::exit(1);
                }
                if removed.is_empty() {
                    println!("tooned: nothing to remove for any agent");
                } else {
                    println!("tooned: removed hooks for {}", removed.join(", "));
                }
            }
            None => {
                eprintln!(
                    "tooned hook uninstall: specify exactly one of --all, --claude-code, --codex, --devin, --droid, --opencode, --kilo, or --pi"
                );
                std::process::exit(EXIT_USAGE_ERROR);
            }
        },
        HookCommand::Status { agent } => match resolve_agent_with_all(agent) {
            Some(Agent::ClaudeCode) => {
                let installed = claude_code::status();
                println!(
                    "tooned: Claude Code hook is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::Codex) => {
                let installed = codex::status();
                println!(
                    "tooned: Codex hook is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::Devin) => {
                let installed = devin::status();
                println!(
                    "tooned: Devin hook is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::Droid) => {
                let installed = droid::status();
                println!(
                    "tooned: Droid hook is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::OpenCode) => {
                let installed = opencode::status();
                println!(
                    "tooned: OpenCode plugin is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::Kilo) => {
                let installed = kilo::status();
                println!(
                    "tooned: Kilo plugin is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::Pi) => {
                let installed = pi::status();
                println!(
                    "tooned: Pi extension is {}",
                    if installed { "installed" } else { "not installed" }
                );
            }
            Some(Agent::All) => {
                for a in &ALL_AGENTS {
                    let installed = status_agent(a);
                    println!(
                        "tooned: {} is {}",
                        agent_display_name(a),
                        if installed { "installed" } else { "not installed" }
                    );
                }
            }
            None => {
                eprintln!(
                    "tooned hook status: specify exactly one of --all, --claude-code, --codex, --devin, --droid, --opencode, --kilo, or --pi"
                );
                std::process::exit(EXIT_USAGE_ERROR);
            }
        },
        // Read-only across both agents' configs -- never writes (data-model.md).
        HookCommand::Doctor => doctor::run(),
    }
}

#[cfg(test)]
mod tests {
    use super::merge_post_tool_use_entry;

    const OLD_PATH: &str = "/opt/old/tooned hook run --claude-code";
    const NEW_PATH: &str = "/opt/new/tooned hook run --claude-code";
    const FOREIGN: &str = "/usr/bin/some-other-tool --watch";

    fn post_tool_use_root(commands: &[&str]) -> serde_json::Value {
        let entries = commands
            .iter()
            .map(|c| {
                serde_json::json!({
                    "matcher": "Bash|Read",
                    "hooks": [ { "type": "command", "command": c } ],
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({ "hooks": { "PostToolUse": entries } })
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn suffix_dedup_collapses_reinstall_with_new_prefix() {
        // Regression: an existing tooned entry under an old absolute path must
        // not cause a duplicate when `tooned` later moves on PATH (reinstall).
        let mut root = post_tool_use_root(&[OLD_PATH]);
        let appended = merge_post_tool_use_entry(&mut root, "Bash|Read", NEW_PATH);
        assert!(!appended, "must not append a duplicate tooned entry");
        let arr = root["hooks"]["PostToolUse"].as_array().expect("array");
        assert_eq!(arr.len(), 1, "exactly one tooned entry should remain");
        assert_eq!(
            arr[0]["hooks"][0]["command"].as_str().expect("command"),
            NEW_PATH,
            "the pre-existing entry must be updated to the new path"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn suffix_dedup_never_touches_foreign_entry() {
        // A foreign tool's entry must never be collapsed by the tooned
        // suffix match: merging a brand-new tooned entry alongside a lone
        // foreign entry appends (does not touch the foreign row).
        let mut root = post_tool_use_root(&[FOREIGN]);
        let appended = merge_post_tool_use_entry(&mut root, "Bash|Read", NEW_PATH);
        assert!(appended, "foreign entry must not block a genuine new insert");
        let arr = root["hooks"]["PostToolUse"].as_array().expect("array");
        assert_eq!(arr.len(), 2, "foreign + new tooned = 2 entries");
        assert_eq!(
            arr[0]["hooks"][0]["command"].as_str().expect("command"),
            FOREIGN,
            "the foreign entry must be left untouched"
        );
    }

    #[test]
    #[allow(clippy::expect_used)]
    fn exact_command_dedup_still_works() {
        let mut root = post_tool_use_root(&[OLD_PATH]);
        let appended = merge_post_tool_use_entry(&mut root, "Bash|Read", OLD_PATH);
        assert!(!appended, "exact command match must still dedupe");
        let arr = root["hooks"]["PostToolUse"].as_array().expect("array");
        assert_eq!(arr.len(), 1);
    }
}
