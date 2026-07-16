# feat(index): replace polling `index watch` with a notify-based debounced watcher

- **Date:** 2026-07-16
- **Author:** w0wl0lxd
- **Branch:** `003-notify-index-watch`
- **PR(s):** TBD

## Context

PR1 from the tooned-next-4-prs plan: replace the temporary polling
`tooned index watch` implementation with a cross-platform, debounced
filesystem watcher. The previous implementation repeatedly ran
`index sync` on a fixed `interval` and was noted as a placeholder.

## Reasoning

`notify` is the standard cross-platform Rust filesystem-watcher crate and
`notify-debouncer-mini` gives us a debounced event stream with a
configurable quiet period. This removes the fixed polling delay, avoids
unnecessary `sync` runs, and keeps the `tooned-index` dependency set
reasonable (it already carries `ignore`/`rusqlite`, so the watcher is
not on the hot conversion path).

Key design choices:
- `tooned-index/src/lib.rs` owns the watcher loop and `sync` dispatch.
- `tooned-cli` exposes `--debounce-ms` and reads `watch.debounce_ms` from
  the TOML config.
- Events under `.tooned/` and `.git/` are filtered before `sync` to avoid
  feedback loops and noise.
- The project `.gitignore` is loaded where possible to further suppress
  irrelevant events.
- A `watch_with_stop(&AtomicBool)` variant is exposed so tests can stop
  the loop cleanly.

## Steps taken

1. Added `notify = "8.2"` and `notify-debouncer-mini = "0.7"` to
   `crates/tooned-index/Cargo.toml`.
2. Replaced `tooned-index/src/lib.rs::watch` with `notify-debouncer-mini`
   and added `watch_with_stop`.
3. Added `watch.debounce_ms` to `crates/tooned-cli/src/config.rs`.
4. Updated `crates/tooned-cli/src/cli/index.rs` to use `--debounce-ms`
   and call `tooned_index::watch`.
5. Added `crates/tooned-index/tests/watch.rs` integration test that
   creates a file while the watcher is running and asserts it gets
   synced into the index.
6. Ran `cargo fmt`, `cargo clippy --all-features --all-targets`, and
   `cargo nextest run --all-features`.
7. Updated `CHANGELOG.md` to describe the new watcher behavior and link
   to this work-log.

## Verification

- `cargo fmt --all -- --check` — PASS
- `cargo clippy --all-features --all-targets -- -D warnings` — PASS
- `cargo nextest run --all-features` — **247 passed, 1 skipped**
- `cargo vet` — pending supply-chain review (see Follow-ups)

## PR description

See PR body for full `## Why`, `## What changed`, `## Verification`,
`## Risk and rollback`, `## Work log`, and `## Changelog` sections.

## Changelog

Updated the `### Added` bullet in `CHANGELOG.md` (line 88) that describes
`index compact`, `index watch`, and `diff` to reflect that `index watch`
is now `notify`-based and debounced. The bullet links back to this
work-log file.

## Follow-ups

- `cargo vet` currently reports unvetted `notify` transitive dependencies.
  This needs either supply-chain audits, publisher trusts, or explicit
  exemptions before the PR can pass the `security.yml` `cargo vet --locked`
  gate. After the PR is opened, add the chosen supply-chain coverage and
  then continue to PR2 (TRON record-stream encoding).
