// SPDX-License-Identifier: AGPL-3.0-only

//! Pi extension wrapper: `tooned hook run --pi`, `hook install --pi`.
//!
//! Pi loads TypeScript extensions from `.pi/extensions/` (project) and
//! `~/.pi/agent/extensions/` (user). The generated extension calls
//! `tooned hook run --pi` with a Claude-compatible `tool_output` payload
//! and returns a patched `content` array when TOON is smaller.
//! See <https://pi.dev/docs/latest/extensions>.

use std::path::Path;
use std::path::PathBuf;

use super::plugin::PluginAgent;
use super::{InstallError, Scope};

fn home_root() -> Option<PathBuf> {
    for var in ["HOME", "USERPROFILE"] {
        if let Some(v) = std::env::var_os(var)
            && !v.is_empty()
        {
            return Some(PathBuf::from(v));
        }
    }
    None
}

const AGENT: PluginAgent = PluginAgent {
    run_flag: "hook run --pi",
    project_dir: ".pi/extensions",
    project_file: "tooned.ts",
    user_root: home_root,
    user_dir: ".pi/agent/extensions",
    user_file: "tooned.ts",
    content: pi_content,
};

fn pi_content(binary: &Path) -> String {
    let path_json = sonic_rs::to_string(&binary.display().to_string())
        .unwrap_or_else(|_| "\"tooned\"".to_string());

    format!(
        r#"// @ts-nocheck
import {{ spawnSync }} from "node:child_process";

const TOONED_BIN = {path_json};

export default function (pi: any) {{
  pi.on("tool_result", async (event: any) => {{
    if (event.isError) {{
      return;
    }}
    const text = (event.content || [])
      .filter((item: any) => item && item.type === "text" && typeof item.text === "string")
      .map((item: any) => item.text)
      .join("\n");
    if (!text) {{
      return;
    }}
    const result = spawnSync(TOONED_BIN, ["hook", "run", "--pi"], {{
      input: JSON.stringify({{
        tool_name: event.toolName,
        tool_input: event.input,
        tool_output: text,
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
      if (typeof updated === "string" && updated.length < text.length) {{
        return {{
          content: [{{ type: "text", text: updated }}],
          details: event.details,
          isError: event.isError,
        }};
      }}
    }} catch {{
      // passthrough
    }}
  }});
}}
"#
    )
}

/// Runs the Pi hook against stdin. The Pi extension calls
/// `tooned hook run --pi` with a Claude-compatible `tool_output` payload,
/// so the runtime path is identical to `--claude-code`.
pub fn run_hook() {
    super::run_hook_protocol(super::HookProtocol::Pi);
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

pub(crate) fn doctor_report() -> serde_json::Value {
    super::plugin::doctor_report(&AGENT)
}
