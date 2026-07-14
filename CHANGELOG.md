# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial workspace scaffold: `tooned-core`, `tooned-index`, `tooned-cli`.
- Adaptive TOON-vs-JSON conversion pipeline (`tooned-core`).
- The `.tooned/` project index (`tooned-index`).
- CLI surface: `convert`, `check`, `pipe`, `wrap`, `index`, `stats`.
- Claude Code and Codex CLI hook integration, installed via `tooned hook install`.
- MCP server (`tooned mcp serve`).
- Dual licensing (AGPL-3.0-only + commercial), mirroring `toon-lsp`.
