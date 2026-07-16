# Cargo-rail monorepo tooling and workspace dependency unification

- **Date:** 2026-07-16
- **Author:** w0wl0lxd
- **Branch:** `cargo-rail-setup`
- **PR(s):** (to be opened from `cargo-rail-setup`)

## Context

The workspace had uncommitted build-tooling scaffolding (`justfile`, `.config/rail.toml`) and a nightly-only `rust-toolchain.toml` override. The dependency graph was decentralized: each crate declared its own versions for internal workspace crates and several shared dev/prod dependencies, making cross-crate version bumps error-prone.

## Reasoning

`cargo-rail` provides deterministic change-planning (`cargo rail plan`/`run`), workspace dependency unification (`cargo rail unify`), and release automation (`cargo rail release`) without adding network-capable dependencies or a heavy external toolchain. Centralizing dependency declarations in `[workspace.dependencies]` and inheriting them with `workspace = true`:

- keeps version bumps in a single location,
- prevents accidental drift between crates,
- aligns with the existing `[workspace.lints]` centralization pattern, and
- leaves feature selections untouched (they stay per-crate).

A `justfile` was added to give contributors a single entrypoint for the release gate (`just validate`) and individual recipes that match the commands in `README.md` and `AGENTS.md`.

## Steps taken

1. Verified the existing `.config/rail.toml` and `justfile` content.
2. Ran `cargo rail unify --check` to preview the change graph; `cargo-rail` reported zero resolved-dependency additions or removals.
3. Applied unification with `cargo rail unify --backup` and inspected the resulting `Cargo.toml` diffs.
4. Formatted all modified `Cargo.toml` files with `taplo fmt`.
5. Updated `CHANGELOG.md` with a build-tooling bullet and a reverse link to this work-log.
6. Re-verified the full release gate on the stable toolchain.

## Verification

```bash
cd /home/w0w/dev/tooned
RUSTUP_TOOLCHAIN=stable cargo fmt --all -- --check
RUSTUP_TOOLCHAIN=stable cargo clippy --all-features --all-targets -- -D warnings
RUSTUP_TOOLCHAIN=stable cargo nextest run --all-features
RUSTUP_TOOLCHAIN=stable RUSTDOCFLAGS=-Dwarnings cargo doc --no-deps --all-features
cargo deny check
cargo audit
```

Observed output:

- `cargo fmt --all -- --check` — PASS (exit 0)
- `cargo clippy --all-features --all-targets -- -D warnings` — PASS (exit 0)
- `cargo nextest run --all-features` — `242 tests run: 242 passed, 1 skipped`
- `cargo doc --no-deps --all-features` — PASS (exit 0)
- `cargo deny check` — `advisories ok, bans ok, licenses ok, sources ok`
- `cargo audit` — PASS (no vulnerabilities)

## PR description

Adds `cargo-rail` configuration (`.config/rail.toml`) and a `justfile` with standard development recipes, then runs `cargo rail unify` to centralize workspace dependency declarations. No resolved dependency graph changes (zero additions/removals); only `Cargo.toml` manifests are reorganized. All release gates pass on stable.

## Follow-ups

- Push `cargo-rail-setup` and open a PR for review; do not merge until `gh` authentication is restored.
- Re-authenticate `gh` or provide `GH_TOKEN` so the open issue `#1` (XML v2 support, already implemented) can be closed and stale local merged branches can be removed.
- Decide whether to keep or revert the uncommitted `rust-toolchain.toml` change to `nightly`; the project CI uses `dtolnay/rust-toolchain@stable` and the unification work does not require nightly.

## Changelog

Inserted under `### Added` (line **83**) in [CHANGELOG.md](../../../CHANGELOG.md), immediately AFTER the preceding bullet that begins with **Dual licensing (AGPL-3.0-only + commercial).** The CHANGELOG bullet records the reverse reference via the Markdown link `[work-log](docs/agents/work-log/2026-07-16-cargo-rail-setup.md)`.
