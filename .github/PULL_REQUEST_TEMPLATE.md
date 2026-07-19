## What

<!-- One or two sentences describing the change. -->

## Why

<!-- Problem solved or feature enabled. Link an issue if relevant. -->

## Checklist

- [ ] Commits signed (`git commit -s`) — see [CONTRIBUTING.md](../CONTRIBUTING.md)
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-features --all-targets -- -D warnings` passes
- [ ] `cargo nextest run --all-features` passes
- [ ] Test coverage for new behavior; regression test for bug fixes
- [ ] `cargo deny check` passes (if dependencies changed)
- [ ] `changelog.d/<name>.<type>.md` fragment added (or `CHANGELOG_SKIP=1` for non-user-facing changes)
