# 001 Phase 8 convergence hardening and wrap overflow fix

- **Date:** 2026-07-15
- **Author:** w0wl0lxd
- **Branch:** `001-adaptive-toon-conversion`
- **PR(s):** [#3](https://github.com/w0wl0lxd/tooned/pull/3)

## Context

Feature 001 (`adaptive-toon-conversion`) had passed its spec-kit implementation and the Phase 8 convergence tasks were appended to its `tasks.md` after a `speckit-converge` pass. The user then asked to push the work and merge it to `main`, which required an independent review, fixing the review findings, and then PR'ing, validating, and merging.

## Reasoning

The convergence work was safety-critical: it touched in-place file writes, bounded reads, SQLite concurrency, MCP panic isolation, and index sync correctness. An independent `adversarial` + `security-review` subagent was used to avoid the confirmation bias of reviewing our own fresh work. The review found two blockable issues (a `deny.toml` regression and an unhardened `wrap.rs`) and several pre-existing/out-of-scope observations. The blockable issues were fixed and re-validated before opening the PR.

## Steps taken

1. Implemented 001 Phase 8 convergence tasks:
   - `tooned convert --out` now detects same-file/symlink/hardlink destinations and reads the source fully before writing.
   - `read_bounded` caps its initial allocation and uses `saturating_add` for the `take` limit.
   - `.gitignore` appends on Unix use `O_NOFOLLOW`.
   - `sync` includes `size_bytes` in the short-circuit and keeps files in `seen` on transient metadata failures.
   - `open_index` sets a 5-second SQLite busy timeout.
   - MCP `tooned_convert`/`tooned_detect`/`tooned_decode` and all index tools run on `tokio::task::spawn_blocking`.
   - `tooned check` prints `json_bytes`, `toon_bytes`, and `savings_pct` independently.
2. Committed and pushed the 001 convergence fixes and the 002 XML planning spec-kit.
3. Ran an independent `/review` (security + adversarial) agent. The agent re-ran the release gate and identified:
   - `deny.toml` had been overwritten by `project-sync` from `/etc/nixos/projects/canonical-deny.toml`, removing the `AGPL-3.0-only`/`MPL-2.0` allowances and failing `cargo deny check`.
   - `wrap.rs` still used the `cap as u64 + 1` / `Vec::with_capacity(cap.saturating_add(1))` overflow pattern.
4. Restored `deny.toml` to the repo's working version (with workspace license and `toon-lsp`/`colored` allowances).
5. Hardened `wrap.rs` to mirror `io.rs`: capped initial allocation and `saturating_add` for the `take` limit.
6. Re-ran the full release gate (`fmt`, `clippy`, `nextest`, `deny`) and committed/pushed the fixes.
7. Added `CHANGELOG.md` and this work-log per the `changelog-worklog-pr-trail` skill.
8. Post-PR review fixes (CodeRabbit/Gemini/Codex): made `convert --out` and
   `.gitignore` writes use a same-directory temp-file-then-rename pattern, and
   removed the JSON-style structural-depth pre-check from YAML/TOML parsing to
   avoid bracket false positives in strings/comments.

## Verification

```bash
cd /home/w0w/dev/tooned
export RUSTC_WRAPPER="" && export CARGO_TARGET_DIR=/tmp/cargo-target-tooned
cargo fmt --all -- --check
cargo clippy --all-features --all-targets -- -D warnings
cargo nextest run --all-features
cargo deny check
```

Observed output:
- `cargo fmt --all -- --check` — PASS (exit 0)
- `cargo clippy --all-features --all-targets -- -D warnings` — PASS (exit 0)
- `cargo nextest run --all-features` — `185 tests run: 185 passed, 1 skipped`
- `cargo deny check` — `advisories ok, bans ok, licenses ok, sources ok`

## PR description

Closes 001 Phase 8 convergence gaps and the `wrap.rs` overflow finding from the review. Merges the completed `001-adaptive-toon-conversion` branch (including the 002 XML planning spec-kit) into `main`.

## Follow-ups

- Monitor PR checks and CodeRabbit comments; address any new review findings.
- The `project-sync` canonical `deny.toml` at `/etc/nixos/projects/canonical-deny.toml` does not allow `AGPL-3.0-only` or `MPL-2.0`, which `project-sync` will continue to report as drift for `tooned`; reconcile with the NixOS config owner.
- Pre-existing findings noted but not in scope of this PR: `resolve_index_path` is permissive, and `read_input` remains unbounded for `convert --to json` by design.

## Changelog

Inserted under `### Fixed` (line **80**) in [CHANGELOG.md](../../../CHANGELOG.md) — this bullet is the FIRST bullet of the new `### Fixed` subsection. The CHANGELOG bullet records the reverse reference via the Markdown link: [work-log](2026-07-15-001-convergence-and-wrap-hardening.md). The YAML/TOML structural-depth fix was inserted immediately AFTER the preceding bullet that begins with **tooned-cli / tooned-index: closed 001 Phase 8 convergence gaps** at line **90**, also linking back to this work-log.
