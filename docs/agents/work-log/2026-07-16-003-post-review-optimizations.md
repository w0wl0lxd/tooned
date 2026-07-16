# 003 Post-review optimizations

- **Date:** 2026-07-16
- **Author:** w0wl0lxd
- **Branch:** `post-review-optimizations`
- **PR(s):** [#11](https://github.com/w0wl0lxd/tooned/pull/11)

## Context

The previous session ended with a list of follow-ups from an adversarial review and optimization report for the `tooned` workspace. This session addressed the remaining high-priority items: toolchain consistency, dependency cleanup, security hardening, CI/CD improvements, supply-chain auditing, index CLI additions, config-file support, and a prototype ONTO encoding.

## Reasoning

- A pinned `stable` toolchain keeps CI and local builds aligned.
- Replacing `toon-lsp` with `toon-format` was investigated and rejected because `toon-format` quotes `@`-prefixed XML attribute keys differently, breaking the existing XML conversion contract; keeping `toon-lsp` is safer until the upstream codec is split.
- `tooned-index` accepts a caller-supplied project root, so symlink checks on `.tooned/index.db` and the `.gitignore` temp file are necessary to prevent filesystem redirection attacks.
- `cargo vet` and `cargo audit` in CI close the supply-chain audit loop.
- Adding `compact`, `watch`, `diff`, and config-file support makes the index and CLI usable without repeated manual flags.
- ONTO is a low-risk prototype because it is gated behind an explicit `--to onto` direction and falls back to passthrough for non-uniform or nested inputs, while TRON is only a placeholder.

## Steps taken

1. **Toolchain/dependencies**: Re-pinned `rust-toolchain.toml` to `stable`; bumped `tokio` to `1.52.4`; removed the `toon-format` migration after validating it would break XML output.
2. **Security hardening**: Added symlink refusal in `tooned-index` for `.tooned/index.db` and the `.gitignore` temp file; hardened Windows temp-file writes.
3. **XML cleanup**: Removed the redundant structural-depth preflight from `tooned-core/src/xml.rs`; relied on `quick-xml` recursion limits and added adversarial tests.
4. **CI/CD**: Added shell completion and man-page generation (`tooned completions` / `tooned man`) plus release packaging; added `benchmark` and `latency` jobs to `ci.yml`; switched `security.yml` to direct `cargo audit` and added a `cargo vet` gate.
5. **Supply-chain audits**: Initialized `cargo vet`, imported mozilla/google audit sets, and added a `vet` CI job.
6. **Index CLI**: Implemented `tooned index compact` (WAL checkpoint), `tooned index watch` (polling sync with a note about future `notify` watcher), and `tooned diff` (compare JSON original with TOON round-trip via `similar`).
7. **Config support**: Added `tooned-cli/src/config.rs` to load TOML config from `--config`, `TOONED_CONFIG`, `$XDG_CONFIG_HOME/tooned/config.toml`, or `.tooned.toml`; merged config defaults with CLI flags for `convert`, `check`, and `pipe`.
8. **ONTO prototype**: Implemented `tooned-convert/src/onto.rs` with ONTO encoder/decoder for uniform arrays of flat objects, exposed `encode_onto`/`decode_onto`/`maybe_onto`, and added `tooned convert --to onto`; `--to tron` is a placeholder.
9. **CHANGELOG/work-log**: Updated `CHANGELOG.md` under `### Added`, `### Fixed`, and `### Security` with reverse links to this work-log.

## Verification

```bash
cd /home/w0w/dev/tooned
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo deny check
cargo audit --json
cargo machete
cargo vet
```

Observed output:

- `cargo fmt --all` — PASS (exit 0)
- `cargo clippy --all-targets --all-features -- -D warnings` — PASS (exit 0)
- `cargo nextest run --all-features` — `246 tests run: 246 passed, 1 skipped`
- `cargo deny check` — `advisories ok, bans ok, licenses ok, sources ok`
- `cargo audit --json` — `vulnerabilities.found: false, count: 0`
- `cargo machete` — `cargo-machete didn't find any unused dependencies`
- `cargo vet` — `Vetting Succeeded (38 fully audited, 1 partially audited, 251 exempted)`

Manual ONTO smoke test:

```bash
echo '[{"id":0,"name":"row-0","active":true,"score":0},
        {"id":1,"name":"row-1","active":false,"score":1.5}]' | \
  cargo run --quiet -- convert - --to onto
```

Produced the expected `!schema` header and pipe-delimited rows.

## PR description

Addresses the post-review optimization backlog: toolchain pinning, dependency cleanup, symlink hardening for `tooned-index`, XML depth-guard cleanup, CI benchmark/latency/vet/audit gates, and user-facing CLI additions (`index compact`, `index watch`, `diff`, TOML config, prototype ONTO encoding with TRON placeholder). All verification gates pass.

## Follow-ups

- Replace the `index watch` polling loop with a `notify`-based filesystem watcher and debounce.
- Implement the TRON record-stream encoding once its schema is finalized.
- Revisit the `toon-lsp` vs `toon-format` decision when the upstream TOON codec is available as a standalone crate.
- Reduce `cargo vet` exemption backlog over time by importing additional trusted audit sets or certifying high-risk crates.

## Changelog

Inserted under `### Added` (line **88**) in [CHANGELOG.md](../../../CHANGELOG.md), immediately AFTER the preceding bullet that begins with **Monorepo build tooling:**. The CHANGELOG bullets for this session record the reverse reference via the Markdown link `[work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md)`.

Also inserted under `### Fixed` (line **107**) in [CHANGELOG.md](../../../CHANGELOG.md), immediately AFTER the preceding bullet header `### Fixed`. The CHANGELOG bullet for the XML preflight removal records the reverse reference via the Markdown link `[work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md)`.

Also inserted under `### Security` (line **138**) in [CHANGELOG.md](../../../CHANGELOG.md), as the first bullet of the new `### Security` subsection. The CHANGELOG bullet for the symlink hardening records the reverse reference via the Markdown link `[work-log](docs/agents/work-log/2026-07-16-003-post-review-optimizations.md)`.
