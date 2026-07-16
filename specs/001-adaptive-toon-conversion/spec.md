# Feature Specification: Adaptive TOON Conversion for AI Agent Tool-Call Context

**Feature Branch**: `001-adaptive-toon-conversion`
**Created**: 2026-07-13
**Status**: Implemented (2026-07-15)
**Input**: User description: "Build tooned: transparently detect JSON-shaped structured data flowing through AI coding agents' tool-call context (API responses, DB rows, config files) and adaptively re-encode it as TOON whenever that measurably reduces size versus compact JSON — never mutating source files, never requiring hand-authored TOON, always falling back safely to passthrough on any doubt. Complementary to rtk (not a replacement); MVP integration targets are Claude Code, Codex CLI, and an agent-agnostic MCP server."

## Clarifications

### Session 2026-07-13

- Q: Which Claude Code tool categories should tooned's PostToolUse hook intercept in v1 (the installer's matcher scope)? → A: Bash, Read, Grep, WebFetch, and all `mcp__*` tool calls.
- Q: Should `tooned index` auto-add `.tooned/` to the target project's .gitignore the first time it creates the index? → A: Yes, auto-append `.tooned/` if not already ignored.
- Q: Does tooned transmit any data externally in v1 (usage telemetry, crash reports, license check-ins)? → A: No telemetry at all — fully local/offline.
- Q: Should `tooned hook install` verify the `tooned` binary resolves on PATH before writing the hook entry into the agent's config? → A: Yes, verify and abort with a clear error if not found.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Automatic Token Savings During an Agent Session (Priority: P1)

A developer is using an AI coding agent (Claude Code or Codex CLI) to work in their repository. During the session, a tool call (running a shell command, reading a file, calling a web API, or invoking another MCP tool) returns a JSON-shaped result — for example, a large API response, a set of database rows, or a config file's contents. Without the developer doing anything, that result is transparently re-encoded into a more compact form before it reaches the agent's context, whenever doing so is actually smaller than the original. If the payload isn't a good fit for the smaller form, the developer sees the original output exactly as before.

**Why this priority**: This is tooned's entire reason for existing — reducing the context/token cost of agent sessions without the developer changing any habits. Every other capability exists to support, install, or make this visible.

**Independent Test**: Can be fully tested by installing the agent hook, triggering a tool call known to produce a uniform JSON array of objects (e.g., `curl` against a REST API, or a `SELECT` producing rows), and confirming the agent's transcript shows the compacted form while a control tool call returning non-JSON or already-compact content is left untouched.

**Acceptance Scenarios**:

1. **Given** a developer has installed the agent hook, **When** a qualifying tool call returns a uniform JSON array of similarly-shaped objects, **Then** the agent receives a re-encoded, smaller version of that same data instead of the original JSON.
2. **Given** a developer has installed the agent hook, **When** a qualifying tool call returns JSON whose re-encoded form would not be smaller than the original, **Then** the agent receives the original output unchanged.
3. **Given** a developer has installed the agent hook, **When** a qualifying tool call returns non-JSON-shaped content (plain text, binary, malformed JSON), **Then** the agent receives the original output unchanged and the session is not interrupted or delayed noticeably.

---

### User Story 2 - Standalone Command-Line Conversion (Priority: P2)

A developer wants to convert a specific file or a command's output on demand, outside of any agent session — for example, to inspect what the compacted form looks like, to pipe a command's JSON output through the tool before pasting it somewhere, or to wrap an existing command so its output is adaptively compacted.

**Why this priority**: Establishes tooned as a useful tool on its own, independent of agent integrations, and is the foundation the agent hooks and MCP server build on.

**Independent Test**: Can be fully tested by running the CLI directly against a sample file or piping a known JSON payload into it, without any agent or hook installed, and confirming correct conversion or passthrough behavior and a dry-run report mode that doesn't alter output.

**Acceptance Scenarios**:

1. **Given** a JSON file on disk, **When** the developer runs the one-shot conversion command against it, **Then** the compacted (or original, if not smaller) content is printed to standard output and the source file is left unmodified.
2. **Given** a command whose output is JSON-shaped, **When** the developer pipes that output into the tool's composable "pipe" mode, **Then** the adaptively-converted content is written to standard output.
3. **Given** a command that the developer wraps with the tool, **When** that command's captured output is JSON-shaped, **Then** the adaptively-converted content replaces the raw output the developer sees.
4. **Given** any file or piped input, **When** the developer runs the dry-run "check" mode, **Then** a report of detected format, shape, and estimated savings is shown and no converted output is produced.

---

### User Story 3 - Project-Wide Savings Visibility (Priority: P3)

A developer wants to understand, ahead of any agent session, which files or data sources in their project would benefit most from adaptive conversion — for example, to decide whether it's worth enabling the hook at all, or to find unexpectedly large fixture/config files.

**Why this priority**: Provides value independent of any live agent session and builds the foundation (a persistent project index) that later features can reuse, but the product delivers its core value (User Story 1) without it.

**Independent Test**: Can be fully tested by running the index command against a sample project directory and confirming it produces a ranked report of convertible files and their estimated savings, without needing any agent or hook installed.

**Acceptance Scenarios**:

1. **Given** a project directory containing a mix of convertible and non-convertible files, **When** the developer runs the index command, **Then** a persistent record of scanned files, their detected format, and estimated savings is created.
2. **Given** a previously indexed project where some files have changed and others have not, **When** the developer runs the incremental sync command, **Then** only changed files are re-scanned and files that no longer exist are removed from the record.
3. **Given** an indexed project, **When** the developer requests the ranked savings report, **Then** the files or payloads with the greatest estimated savings are shown first, limited to the requested count.

---

### User Story 4 - Safe Installation Alongside Other Agent Tools (Priority: P2)

A developer already has another agent tool (such as rtk) with its own hook registered in their agent configuration. They install tooned's hook/MCP integration and expect both tools to keep working exactly as before, with neither tool's configuration entries lost, duplicated, or corrupted — including when tooned's own installer is run more than once, or when it is later uninstalled.

**Why this priority**: tooned is explicitly positioned to run alongside existing agent tooling rather than replace it; failing this would make tooned unsafe to adopt for anyone with an existing hook-based setup.

**Independent Test**: Can be fully tested by installing another tool's hook entry first, then installing tooned's hook, and confirming both entries are present, correctly formed, and both fire during a session; then running the installer again and uninstalling, confirming no duplication and no loss of the other tool's entry at any step.

**Acceptance Scenarios**:

1. **Given** an agent configuration with an existing hook entry from another tool, **When** the developer installs tooned's hook, **Then** the existing entry is preserved unchanged and tooned's entry is added alongside it.
2. **Given** tooned's hook is already installed, **When** the developer runs the installer again, **Then** no duplicate entry is created.
3. **Given** tooned's hook is installed alongside another tool's entry, **When** the developer uninstalls tooned, **Then** only tooned's entry is removed and the other tool's entry remains intact.
4. **Given** an agent configuration with multiple hook entries, **When** the developer runs the diagnostic command, **Then** it reports the installation status of tooned's own entries without misreporting or altering any other tool's entries.

---

### Edge Cases

- What happens when a tool-call payload is technically valid JSON but represents a single deeply-nested, irregular structure (not a uniform array of records)? → It is still evaluated by the size comparison; converted only if the encoded form is actually smaller.
- What happens when a payload is exactly at, or within the configured margin of, the same size in both forms? → Treated as not-smaller-enough; the original is passed through, to avoid flapping between forms across near-identical payloads.
- What happens when input exceeds the configured maximum size? → It is passed through unconverted without being parsed at all, to bound worst-case processing time.
- What happens when input claims to be one format (e.g., by file extension) but its content doesn't actually match? → Detection falls back to content sniffing; if no supported format is confidently detected, the original is passed through.
- What happens when a converted payload fails to round-trip back to an equivalent value? → The conversion is discarded and the original is passed through; this payload is never surfaced as converted.
- How does the system handle a tool call whose output arrives incomplete or is still streaming? → Only complete, already-captured output is evaluated; partial/streaming content is not processed mid-stream.
- What happens if the project index is queried for a directory that has never been scanned? → The command reports that no index exists yet and does not fail destructively.
- What happens if two installers (tooned's and another tool's) attempt to modify agent configuration at the same time? → Each installer's write is expected to be atomic with respect to the configuration file so that a concurrent run cannot produce a corrupted or partially-written file.
- What happens when a developer runs the uninstaller without ever having installed tooned? → It reports nothing to remove without error.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: System MUST detect, for any tool-call output or CLI input, whether the content is JSON, NDJSON/JSONL, YAML, TOML, or CSV/TSV shaped, without requiring the caller to declare the format, though an explicit format hint MUST be honored when supplied.
- **FR-002**: System MUST parse detected content into a common structured representation before evaluating conversion.
- **FR-003**: System MUST compute the size of the re-encoded representation and the size of the equivalent compact (non-pretty) JSON representation for every detected payload before deciding whether to convert.
- **FR-004**: System MUST return the re-encoded representation only when it is smaller than the compact JSON representation by more than a configurable margin (default 2%); otherwise it MUST return the original content unchanged.
- **FR-005**: System MUST NOT mutate any source file; all conversion happens on data in transit (tool-call output, piped stdin, or an explicitly requested one-shot conversion).
- **FR-006**: System MUST fall back to passing the original, unmodified content through whenever parsing fails, the input exceeds a configurable maximum size (default 2 MiB), or any other error or ambiguity occurs during detection, parsing, or conversion.
- **FR-007**: System MUST NOT crash, hang, or block the invoking agent's tool call under any input, including malformed, truncated, or adversarial content.
- **FR-008**: System MUST verify that every payload it converts can be decoded back to an equivalent structured value; a payload that fails this check MUST NOT be surfaced as converted.
- **FR-009**: System MUST provide a standalone command-line interface for one-shot conversion of a file or stdin stream, independent of any agent session, that never mutates the source.
- **FR-010**: System MUST provide a standalone dry-run/check mode that reports a payload's detected format, shape classification, and estimated savings without producing converted output.
- **FR-011**: System MUST provide a composable "pipe" mode that reads from stdin and adaptively writes converted-or-original content to stdout, suitable for chaining after another command's output.
- **FR-012**: System MUST provide a "wrap" mode that runs a given command and adaptively converts its captured stdout when it is JSON-shaped.
- **FR-013**: System MUST integrate with Claude Code's tool-call hook mechanism to inspect, and when beneficial replace, the output of qualifying tool calls after they complete, without altering the invoking tool call itself. In v1, qualifying tool calls are: shell command execution, file reads, search/grep, web fetches, and any MCP tool call.
- **FR-014**: System MUST integrate with Codex CLI's equivalent tool-output hook mechanism to provide the same adaptive conversion behavior after a qualifying tool call completes.
- **FR-015**: System MUST expose its conversion, detection, decoding, and indexing capabilities through an agent-agnostic protocol server, so any compatible agent can invoke them directly without a native hook integration.
- **FR-016**: System MUST provide an installer that adds its own hook/integration registration to a developer's agent configuration idempotently — running the installer multiple times MUST NOT create duplicate entries. Before writing any hook entry, the installer MUST verify that the tool it is registering resolves to a runnable binary, and MUST abort with a clear, actionable error instead of writing a hook entry that would silently never fire.
- **FR-017**: System's installer MUST preserve any existing entries already present in the developer's agent configuration, including entries belonging to other tools, and MUST NOT overwrite the configuration wholesale.
- **FR-018**: System MUST provide an uninstaller that removes only its own previously-installed entries, leaving all other configuration untouched.
- **FR-019**: System MUST provide a diagnostic command that reports the current state of its own and any other detected relevant installations, so a developer can confirm tooned and other tools are both correctly installed.
- **FR-020**: System MUST provide a project-level index command that scans a directory for convertible files, classifies their shape, and records an estimated savings report, without requiring an active agent session. The first time it creates the index for a project, it MUST ensure the index's storage location is excluded from that project's version control (e.g., appending it to .gitignore if not already covered) rather than leaving it to the developer to discover.
- **FR-021**: System's index MUST support incremental updates that skip re-scanning files whose modification time and content are unchanged since the last scan, and MUST remove index entries for files that no longer exist.
- **FR-022**: System MUST provide a command that reports the ranked, highest-savings-opportunity files or payloads found by the index, limited to a developer-specified count.
- **FR-023**: System MUST support an opt-in precise savings-estimation mode based on actual language-model tokenization, distinct from and never substituted into the default size-based decision path.
- **FR-024**: System MUST clearly document, for the current release, which structured data formats and which agent/editor integrations are supported versus explicitly out of scope.
- **FR-025**: System MUST NOT transmit tool-call payload content, file content, or usage data to any external service. All detection, parsing, conversion, and indexing MUST happen entirely on the developer's own machine, with no telemetry, crash reporting, or license check-in network calls in v1.

### Key Entities

- **Conversion Decision**: The outcome of evaluating one payload — which format was detected, whether it was converted or passed through, and the size comparison that produced that outcome.
- **Payload Shape Profile**: A classification of a structured payload's regularity (e.g., a uniform collection of similarly-shaped records versus an irregular or deeply nested structure), used as context for the conversion decision.
- **Project Index**: A persistent, per-project record of previously scanned files, their detected format, shape profile, and cached savings estimate, enabling offline savings visibility without a live agent session.
- **Integration Installation Record**: The set of configuration entries tooned has added to a given agent's setup, tracked so install, uninstall, and diagnostic operations remain precise and never disturb entries belonging to other tools.
- **Agent Integration Surface**: One of the supported ways a developer or agent invokes tooned's conversion capability — the standalone CLI, the Claude Code hook, the Codex CLI hook, or the agent-agnostic protocol server.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: For representative uniform, tabular JSON payloads (the kind typical of API responses and database query results), payloads chosen for conversion are reduced in size versus compact JSON in 100% of cases where a conversion is applied — the system never presents an enlarged result as a "savings".
- **SC-002**: A developer sees no perceptible added delay from tooned during a normal agent session: the added processing time for a typical tool-call payload (around 100 KB) stays under 5 milliseconds.
- **SC-003**: Malformed, oversized, or otherwise ambiguous tool-call output reaches the agent unchanged in 100% of observed cases — no dropped, corrupted, or hung tool call is ever attributable to tooned.
- **SC-004**: Installing tooned's agent integration alongside an existing hook-based tool (such as rtk) never removes, duplicates, or otherwise disturbs that other tool's configuration entries, verified by comparing the configuration before and after install/uninstall.
- **SC-005**: A developer can obtain a ranked, project-wide savings-opportunity report for a typical repository (up to several thousand files) in well under a minute, and a repeat run after only a handful of files changed completes noticeably faster than the initial full scan.
- **SC-006**: A developer new to tooned can install it, run a first successful conversion, and install an agent integration using only the tool's own command-line help output, without consulting external documentation.

## Assumptions

- toon-lsp's existing encode/decode functionality is stable enough to serve as tooned's underlying codec without requiring changes to that project.
- The developer's AI coding agent (Claude Code, Codex CLI) already provides a working hook or tool-integration mechanism, and the developer has permission to modify their own local agent configuration.
- Byte length is an acceptable default proxy for the token cost a large language model would incur; exact tokenizer-based measurement is valuable but only as an explicit opt-in, not the default decision path.
- v1 targets JSON, NDJSON/JSONL, YAML, TOML, and CSV/TSV as input formats; other formats (e.g., XML) are out of scope for this release and are tracked separately.
- v1 targets Claude Code, Codex CLI, and an agent-agnostic protocol server as integration surfaces; broader editor/agent coverage is out of scope for this release.
- tooned and other agent tooling (notably rtk) may be installed simultaneously on the same machine and agent configuration; no explicit coordination protocol between the two is required beyond each tool safely managing only its own configuration entries.
- Developers installing tooned either have a working Rust toolchain (for `cargo install`) or can use a prebuilt binary; no additional runtime (e.g., Python, Node.js) is introduced as a hard dependency.
