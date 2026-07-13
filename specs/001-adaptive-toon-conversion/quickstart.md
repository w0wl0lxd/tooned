# Quickstart: tooned

## Install

```bash
cargo install tooned-cli
# or a prebuilt binary from the GitHub release, once published
```

## Try it standalone (no agent needed)

```bash
curl -s https://api.example.com/users | tooned pipe
# prints TOON if smaller than compact JSON, otherwise the original JSON unchanged

tooned check some-api-response.json
# doc type, shape, estimated savings — no output produced

tooned convert config.yaml --to toon
```

## See project-wide savings potential

```bash
tooned index .
tooned stats --top 10
```

## Wire it into an agent session

```bash
# Claude Code
tooned hook install --claude-code --scope project

# Codex CLI (writes a .codex-plugin/ bundle; also registers the MCP server)
tooned hook install --codex --mcp
# then, inside Codex CLI: /hooks   (required trust review before it will fire)
```

Confirm both tooned and any other hook-based tool (e.g. rtk) are correctly
installed side by side:

```bash
tooned hook doctor
```

## Uninstall

```bash
tooned hook uninstall --claude-code --scope project
tooned hook uninstall --codex
```

Uninstalling never touches another tool's hook entries — only tooned's own.
