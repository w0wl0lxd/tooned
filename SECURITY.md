# Security Policy

## Reporting a Vulnerability

Please report security vulnerabilities privately rather than opening a public issue.

- **Email**: w0wl0lxd@tuta.com
- **GitHub**: [Private vulnerability report](https://github.com/w0wl0lxd/tooned/security/advisories/new)

Include a description of the vulnerability, steps to reproduce, and its potential
impact. You should receive an initial response within a few business days.

## Scope

tooned runs entirely locally: it has no telemetry and makes no external network
calls in v1 (see `specs/001-adaptive-toon-conversion/spec.md`, FR-025). Security
reports of particular interest include:

- Agent hook installers (`tooned hook install`) writing or corrupting a developer's
  Claude Code / Codex CLI configuration in an unsafe way.
- The MCP server (`tooned mcp serve`) accepting malformed input in a way that
  panics, hangs, or otherwise misbehaves.
- Any path where the `.tooned/` project index or the conversion pipeline could be
  made to read or write outside the intended project directory.

## Supported Versions

Pre-1.0: only the latest published release is supported with security fixes.
