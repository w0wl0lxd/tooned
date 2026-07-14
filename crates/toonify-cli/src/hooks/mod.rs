//! `tooned hook` subcommands: `run`, `install`, `uninstall`, `status`,
//! `doctor`, for both Claude Code and Codex CLI.
//! See `specs/001-adaptive-toon-conversion/contracts/{claude-code-hook,codex-hook}.md`.

pub mod claude_code;
pub mod codex;
pub mod doctor;

use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Scope {
    User,
    Project,
}

/// Exactly one of `--claude-code` / `--codex` selects the target agent.
#[derive(Debug, Args)]
pub struct AgentSelector {
    #[arg(long = "claude-code", group = "agent")]
    pub claude_code: bool,

    #[arg(long = "codex", group = "agent")]
    pub codex: bool,
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
        agent: AgentSelector,

        #[arg(long, value_enum)]
        scope: Option<Scope>,

        #[arg(long)]
        mcp: bool,
    },
    /// Removes only tooned's own entries.
    Uninstall {
        #[command(flatten)]
        agent: AgentSelector,

        #[arg(long, value_enum)]
        scope: Option<Scope>,
    },
    /// Reports whether tooned's hook is currently installed.
    Status {
        #[command(flatten)]
        agent: AgentSelector,
    },
    /// Reports all detected hook installations (tooned's and others') for both agents.
    Doctor,
}

/// Which agent an [`AgentSelector`] resolved to; `None` when neither or both
/// flags were passed (clap's `group` tagging on `AgentSelector` doesn't by
/// itself enforce "exactly one" for a flattened `Args` struct, so this is
/// re-validated here rather than trusted).
enum Agent {
    ClaudeCode,
    Codex,
}

fn resolve_agent(agent: &AgentSelector) -> Option<Agent> {
    match (agent.claude_code, agent.codex) {
        (true, false) => Some(Agent::ClaudeCode),
        (false, true) => Some(Agent::Codex),
        _ => None,
    }
}

/// Exit code used across `hook install`/`hook run` for conditions that
/// aren't a payload-driven passthrough decision (contracts/cli.md).
const EXIT_USAGE_ERROR: i32 = 2;
const EXIT_BINARY_NOT_ON_PATH: i32 = 4;

/// Matchers exactly as specified by the contracts (verified, not guessed --
/// see `specs/001-adaptive-toon-conversion/contracts/claude-code-hook.md`
/// and `contracts/codex-hook.md`).
pub(crate) const CLAUDE_CODE_MATCHER: &str = "Bash|Read|Grep|WebFetch|^mcp__";
pub(crate) const CODEX_MATCHER: &str = "Bash";

/// Errors that can occur while installing a hook. Never surfaces as a panic;
/// `hooks::run` maps this to a clear stderr message and the exit code
/// `contracts/cli.md` documents (4 for `BinaryNotOnPath`, 1 otherwise).
#[derive(Debug, thiserror::Error)]
pub(crate) enum InstallError {
    #[error(
        "could not resolve a `tooned` binary on PATH; install it first \
         (e.g. `cargo install tooned-cli`, or a prebuilt release binary) so it is \
         discoverable on PATH before running `tooned hook install`"
    )]
    BinaryNotOnPath,
    #[error(
        "could not determine a home directory for --scope user \
         (neither $HOME nor %USERPROFILE% is set)"
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

/// Parses `path` as a JSON object, tolerating a missing/unreadable/malformed
/// file by starting fresh (`{}`) rather than erroring -- the installer's own
/// job is to merge in a hook entry, not to validate the rest of an agent's
/// config file.
pub(crate) fn read_json_value(path: &Path) -> serde_json::Value {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => serde_json::json!({}),
        },
        Err(_) => serde_json::json!({}),
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
    let text = serde_json::to_string_pretty(value).map_err(|e| {
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

    if arr.iter().any(|entry| entry_has_command(entry, command)) {
        return false;
    }

    arr.push(serde_json::json!({
        "matcher": matcher,
        "hooks": [ { "type": "command", "command": command } ],
    }));
    true
}

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

/// Command suffixes that identify tooned's own `PostToolUse` entries,
/// independent of the absolute binary path prefix (which may legitimately
/// differ between an `install` and a later `uninstall`/`status` run, e.g.
/// after a reinstall to a new location) -- see data-model.md's "Integration
/// Installation Record" identity rules (FR-016/FR-018).
pub(crate) const CLAUDE_CODE_COMMAND_SUFFIX: &str = "hook run --claude-code";
pub(crate) const CODEX_COMMAND_SUFFIX: &str = "hook run --codex";

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
}

impl HookProtocol {
    /// The stdin JSON field carrying the tool's raw result text.
    fn input_field(self) -> &'static str {
        match self {
            HookProtocol::ClaudeCode => "tool_output",
            HookProtocol::Codex => "tool_response",
        }
    }
}

/// Reads a `PostToolUse` stdin payload (per `contracts/claude-code-hook.md`
/// / `contracts/codex-hook.md`), extracts the tool's raw output (stdin field
/// name depends on `protocol`), and runs it through
/// [`tooned_core::maybe_tooned`]. Returns the JSON string to print to
/// stdout on a convert decision, or `None` for passthrough (passthrough
/// means "print nothing" per both contracts, not echoing the original bytes
/// back out -- the host platform already preserves the original tool output
/// whenever the hook prints nothing).
///
/// The emitted `hookSpecificOutput` shape also depends on `protocol`: Claude
/// Code supports replacing the tool's output in place via
/// `updatedToolOutput`, but Codex's real output parser has no such field --
/// it only recognizes `hookSpecificOutput.additionalContext` for surfacing
/// extra content, so that's what's emitted for `HookProtocol::Codex`.
///
/// Never panics for any `stdin` byte slice, including invalid UTF-8 or
/// malformed/adversarial JSON -- every fallible step folds into `None`
/// rather than propagating an error or panicking (constitution Principle I).
/// Callers additionally wrap this in `std::panic::catch_unwind` as
/// defense-in-depth (see `claude_code::run_hook`/`codex::run_hook`).
pub(crate) fn process_hook_stdin(stdin: &[u8], protocol: HookProtocol) -> Option<String> {
    let payload: serde_json::Value = serde_json::from_slice(stdin).ok()?;
    let tool_output = payload.get(protocol.input_field())?;
    let bytes: Vec<u8> = match tool_output {
        serde_json::Value::String(s) => s.as_bytes().to_vec(),
        other => serde_json::to_vec(other).ok()?,
    };

    let opts = tooned_core::ConversionOptions::default();
    let conversion = tooned_core::maybe_tooned(&bytes, &opts).ok()?;
    match conversion {
        tooned_core::Conversion::Toon { text, .. } => {
            let out = match protocol {
                HookProtocol::ClaudeCode => serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PostToolUse",
                        "updatedToolOutput": text,
                    }
                }),
                HookProtocol::Codex => serde_json::json!({
                    "hookSpecificOutput": {
                        "hookEventName": "PostToolUse",
                        "additionalContext": text,
                    }
                }),
            };
            serde_json::to_string(&out).ok()
        }
        tooned_core::Conversion::Passthrough { .. } => None,
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
                // No/ambiguous agent selection on `hook run` is itself a
                // form of doubt -- the contract's fail-safe exit-0 guarantee
                // applies uniformly, not just to payload-driven failure.
                None => {}
            }
            // Contract: `hook run` ALWAYS exits 0, regardless of internal
            // outcome -- a non-zero exit is itself a form of "loud failure"
            // the fail-safe principle forbids (contracts/claude-code-hook.md,
            // contracts/codex-hook.md).
            std::process::exit(0);
        }
        HookCommand::Install { agent, scope, mcp } => match resolve_agent(agent) {
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
            None => {
                eprintln!("tooned hook install: specify exactly one of --claude-code or --codex");
                std::process::exit(EXIT_USAGE_ERROR);
            }
        },
        HookCommand::Uninstall { agent, scope } => match resolve_agent(agent) {
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
            None => {
                eprintln!(
                    "tooned hook uninstall: specify exactly one of --claude-code or --codex"
                );
                std::process::exit(EXIT_USAGE_ERROR);
            }
        },
        HookCommand::Status { agent } => match resolve_agent(agent) {
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
            None => {
                eprintln!("tooned hook status: specify exactly one of --claude-code or --codex");
                std::process::exit(EXIT_USAGE_ERROR);
            }
        },
        // Read-only across both agents' configs -- never writes (data-model.md).
        HookCommand::Doctor => doctor::run(),
    }
}
