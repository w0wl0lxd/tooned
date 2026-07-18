// SPDX-License-Identifier: AGPL-3.0-only

//! OpenCode plugin wrapper: `tooned hook run --opencode`,
//! `hook install --opencode`.
//!
//! OpenCode loads TypeScript plugins from `.opencode/plugins/` (project) and
//! `~/.config/opencode/plugins/` (user). The generated plugin calls
//! `tooned hook run --opencode` with a Claude-compatible `tool_output` payload
//! and mutates `output.output` in place when TOON is smaller.
//! See <https://opencode.ai/docs/plugins>.

use std::path::Path;
use std::path::PathBuf;

use super::plugin::PluginAgent;
use super::{InstallError, Scope};

fn config_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA").filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(appdata));
        }
        // Fall back to $HOME/.config for test environments that clear %APPDATA%.
    }
    let home =
        std::env::var_os("HOME").or(std::env::var_os("USERPROFILE")).filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join(".config"))
}

const AGENT: PluginAgent = PluginAgent {
    run_flag: "hook run --opencode",
    project_dir: ".opencode/plugins",
    project_file: "tooned.ts",
    user_root: config_root,
    user_dir: "opencode/plugins",
    user_file: "tooned.ts",
    content: opencode_content,
};

fn opencode_content(binary: &Path) -> String {
    let path_json = sonic_rs::to_string(&binary.display().to_string())
        .unwrap_or_else(|_| "\"tooned\"".to_string());

    format!(
        r#"// @ts-nocheck
import {{ spawnSync }} from "node:child_process";

const TOONED_BIN = {path_json};

export default function (_input: any) {{
  return {{
    "tool.execute.after": async (toolInput: any, toolOutput: any) => {{
      if (!toolOutput.output || typeof toolOutput.output !== "string") {{
        return;
      }}
      const result = spawnSync(TOONED_BIN, ["hook", "run", "--opencode"], {{
        input: JSON.stringify({{
          tool_name: toolInput.tool,
          tool_input: toolInput.args,
          tool_output: toolOutput.output,
        }}),
        encoding: "utf-8",
        maxBuffer: 16 * 1024 * 1024,
        timeout: 5000,
      }});
      if (result.error || result.status !== 0) {{
        return;
      }}
      const trimmed = (result.stdout || "").trim();
      if (!trimmed) {{
        return;
      }}
      try {{
        const parsed = JSON.parse(trimmed);
        const updated = parsed?.hookSpecificOutput?.updatedToolOutput;
        if (typeof updated === "string" && updated.length < toolOutput.output.length) {{
          toolOutput.output = updated;
        }}
      }} catch {{
        // passthrough
      }}
    }},
  }};
}}
"#
    )
}

/// Runs the OpenCode hook against stdin. The OpenCode plugin calls
/// `tooned hook run --opencode` with a Claude-compatible `tool_output` payload,
/// so the runtime path is identical to `--claude-code`.
pub fn run_hook() {
    super::run_hook_protocol(super::HookProtocol::OpenCode);
}

pub fn install(scope: Option<Scope>, _mcp: bool) -> Result<(), InstallError> {
    super::plugin::install(&AGENT, scope)
}

pub fn uninstall(scope: Option<Scope>) -> Result<bool, InstallError> {
    super::plugin::uninstall(&AGENT, scope)
}

pub fn status() -> bool {
    super::plugin::status(&AGENT)
}

pub(crate) fn target_path(scope: Option<Scope>) -> Result<std::path::PathBuf, InstallError> {
    super::plugin::settings_path(&AGENT, super::plugin::default_scope(scope))
}

pub(crate) fn doctor_report() -> serde_json::Value {
    super::plugin::doctor_report(&AGENT)
}
