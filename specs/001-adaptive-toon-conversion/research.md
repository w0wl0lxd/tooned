# Phase 0 Research: Adaptive TOON Conversion for AI Agent Tool-Call Context

All items below were open technical facts (not product decisions) at planning time.
Each was resolved via live documentation research rather than assumed from the
original product-context document, since several of that document's claims turned
out to be stale or incorrect.

## 1. Claude Code `PostToolUse` hook contract

**Decision**: Register `PostToolUse` hooks in `settings.json` under `hooks.PostToolUse`
as an array of `{matcher, hooks: [{type: "command", command, timeout?}]}` entries.
Use matcher `Bash|Read|Grep|WebFetch|^mcp__` (per clarification's confirmed scope).
On a convert decision, print to stdout:
```json
{"hookSpecificOutput": {"hookEventName": "PostToolUse", "updatedToolOutput": "<converted text>"}}
```
On no-decision (passthrough), print nothing and exit 0.

**Rationale**: Verified against current official Claude Code docs
(`code.claude.com/docs/en/hooks.md`, `.../agent-sdk/hooks.md`):
- Hook stdin receives `{session_id, prompt_id, transcript_path, cwd, permission_mode, hook_event_name, tool_name, tool_input, tool_output}` — `tool_output` gives tooned the actual bytes it needs (confirms the plan's premise that this must be `PostToolUse`, not `PreToolUse`).
- `hookSpecificOutput.updatedToolOutput` is confirmed as the current, universal (all-tools, not just MCP) field. The older `updatedMCPToolOutput` is deprecated.
- Hook failure modes (non-zero exit, crash, timeout, malformed JSON) are all documented as **fail-open**: the original `tool_output` reaches the model unchanged, satisfying constitution Principle I without tooned needing to do anything extra on the Claude Code side.
- Multiple matcher-grouped hook entries coexist safely in the `PostToolUse` array — this is what makes the installer's "append, don't clobber" JSON-merge strategy (FR-017) mechanically sound.

**Correction to original plan document**: the plan assumed the MCP matcher was a glob
`mcp__.*`/`mcp__*`. Neither is correct. The verified syntax is the **anchored regex**
`^mcp__`, which matches any tool name starting with `mcp__` (e.g. `mcp__memory__search`).
The plan's cited version marker ("v2.1.121") could not be verified in current docs;
current docs describe this behavior as of v2.1.195+. The behavior itself (universal
`updatedToolOutput` field) is otherwise exactly as the plan assumed.

**Alternatives considered**: A `PreToolUse` hook was rejected (per the original spec/plan
reasoning, reconfirmed here) because the conversion decision fundamentally requires the
already-produced output bytes, which `PreToolUse` does not have access to.

## 2. Codex CLI hook contract

**Decision**: Register a `PostToolUse` hook via `hooks.json` (or an inline `[hooks]`
table in `config.toml`), matcher `Bash` for shell-tool output (not `shell`/`exec`/
`local_shell` — none of those are real Codex CLI matcher values). Bundle both the hook
and the MCP server registration in one `.codex-plugin/plugin.json` (via its `hooks` and
`mcpServers` fields) so a single plugin install covers both integration surfaces, per
the original plan's intent.

**Rationale**: Verified against `developers.openai.com/codex/hooks` and
`developers.openai.com/codex/build-plugins`:
- Codex CLI does have a lifecycle hooks system, directly analogous to Claude Code's, supporting `PreToolUse`/`PostToolUse` (plus session/compact/subagent lifecycle events not relevant here).
- The `matcher` field is a regex applied to `tool_name`; the canonical shell tool name is `Bash`. This directly corrects the plan's three guessed candidates (`shell`, `exec`, `local_shell`) — none are correct. (`local_shell` is a real OpenAI term, but names a *Responses API* tool schema, unrelated to the Codex CLI hook matcher — a plausible source of the original confusion.)
- Plugin bundling of `hooks` + `mcpServers` in one `.codex-plugin/plugin.json` is confirmed to exist as documented, matching the plan's design intent exactly.
- Fail-open is **not** blanket-guaranteed by Codex CLI's own docs (only specific failure modes, like a `PreToolUse` hook returning a malformed field, are documented as fail-open). This confirms the plan's own caution was correct: tooned's binary MUST independently guarantee non-blocking behavior (constitution Principle I) rather than relying on the platform.
- Non-managed hooks (which tooned's installer writes) require explicit developer trust review via Codex CLI's `/hooks` command before they execute; the plan's first-run-UX requirement (tell the user to run `/hooks`) is confirmed necessary, not optional polish.

**Alternatives considered**: None — Codex CLI's hook mechanism is a close enough analogue
of Claude Code's that no alternative integration strategy (e.g., wrapping the `codex`
binary) was evaluated.

## 3. MCP server: `rmcp`

**Decision**: Build `tooned mcp serve` on `rmcp` (crates.io, current version 2.2.0),
server-side stdio transport via the `transport-io` feature (`rmcp::transport::io::stdio`),
tools defined via the `#[tool_router(server_handler)]` + `#[tool(...)]` macro pattern.

**Rationale**: Confirmed `rmcp` is the official `modelcontextprotocol/rust-sdk` crate,
actively maintained (~20+ published versions, most recent 2026-07-08), not a
hallucinated pick. Stdio transport is directly supported for exactly this use case.
Also supports Streamable HTTP transports and a generic `Transport` trait, which are not
needed for v1 but leave room for a future non-stdio MCP surface without a rewrite.

**Alternatives considered**: Hand-rolling the MCP JSON-RPC wire protocol was rejected —
`rmcp` is the maintained, spec-compliant reference implementation; reimplementing it
would be pure risk for no benefit.

## 4. JSON parsing: `sonic-rs` vs `serde_json`

**Decision**: Use `sonic_rs::from_slice::<serde_json::Value>(bytes)` for JSON inputs
above a size threshold (exact threshold to be tuned during implementation via
benchmarking, not fixed at planning time), falling back to plain `serde_json::from_slice`
below that threshold and on non-SIMD-accelerated architectures.

**Rationale**: Confirmed `sonic-rs` (current version 0.5.8) now supports stable Rust
(no longer nightly-only, per its own README), and runs on all architectures — SIMD
acceleration is x86_64/aarch64-only, but it is not a hard *requirement* to run at all;
other architectures get a slower pure-Rust fallback rather than failing to compile/run.
`from_slice<T>` is generic over any `T: Deserialize`, confirming
`sonic_rs::from_slice::<serde_json::Value>` works as the plan assumed, and resulting
key order is governed by `serde_json::Value`'s own `preserve_order` feature regardless
of which deserializer fed it.

**Caveat surfaced by this research** (not in the original plan): `sonic-rs`'s own docs
recommend using its native `sonic_rs::Value` instead of feeding `serde_json::Value`,
specifically because the two differ on **duplicate-key handling**. Since TOON's key
uniformity/shape classification depends on trusting object key sets, `tooned-core`'s
detection/parse layer MUST include an explicit test case for duplicate JSON keys to
confirm the chosen path (`sonic_rs::Value` → convert, vs. direct
`sonic_rs::from_slice::<serde_json::Value>`) behaves identically to plain `serde_json`
for that edge case, before the `sonic-rs` fast path ships enabled by default.

**Alternatives considered**: Always using `serde_json` (rejected — leaves SIMD speedup
on the table for larger payloads, where the hot-path latency budget matters most);
always using `sonic-rs` regardless of size (rejected — SIMD setup overhead can lose to
plain `serde_json` on small payloads, and non-SIMD architectures get no benefit from the
extra dependency weight for small inputs).

## 5. `.tooned/` SQLite index storage location & `.gitignore` integration

**Decision**: Store the index at `.tooned/index.db` at the scanned project's root.
On first creation, `tooned-index` appends `.tooned/` to that project's `.gitignore`
(creating the file if absent) if not already covered by an existing ignore rule.

**Rationale**: Directly resolved by clarification session 2026-07-13 (see spec.md
Clarifications). Matches the `.codegraph/`-style local-tooling convention already used
elsewhere in this environment.

**Alternatives considered**: Leaving `.gitignore` untouched (rejected by clarification —
increases risk of a developer accidentally committing a local cache database); an
interactive prompt at first-run (rejected — tooned's CLI commands are expected to be
non-interactive/scriptable by default, consistent with `tooned convert`/`check`/`pipe`
already being pipeline-friendly).
