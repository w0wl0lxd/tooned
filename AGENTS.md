# AGENTS.md

tooned is a Rust workspace (`crates/tooned-core`, `crates/tooned-cli`, `crates/tooned-index`)
that transparently swaps JSON tool-call data for TOON encoding when TOON is smaller, wired in
as a Claude Code hook, a Codex CLI hook, an MCP server, or a standalone CLI/pipe.

## Shared Reasoning Memory (Thoughtbox)

Enforced globally (see `~/.agents/AGENTS.md`) — this project does not opt out. Use the
`thoughtbox` MCP knowledge graph for durable, cross-agent facts specific to this repo: TOON
conversion edge cases and their resolutions, hook-install compatibility findings (Claude Code /
Codex CLI / MCP), and spec-kit (`.specify/`) planning decisions. Ephemeral task reasoning still
belongs in a thoughtbox session; only graduate what should outlive the task into the knowledge
graph.

## User-Facing Text

Enforced globally (see `~/.agents/AGENTS.md`) — this project does not opt out. User-facing text
here is CLI/error-message register only, not UI copy: `eprintln!`/`anyhow::bail!` diagnostics in
`crates/tooned-cli/src/cli/*.rs` follow a `tooned <subcommand>: <message>` prefix, lowercase,
one line. Match that register before adding a new CLI message.

## Attribution

All commits, pull requests, changelogs, work-logs, and other project artifacts
must attribute authorship to the human maintainer only
(`w0wl0lxd <199849635+w0wl0lxd@users.noreply.github.com>`). Do not add
`Co-Authored-By: Devin ...`, `Generated with [Devin](...)`, or any other
AI-agent attribution to commit messages, PR descriptions, comments, or
repository files.
