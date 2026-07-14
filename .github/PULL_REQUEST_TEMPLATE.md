## What

<!-- What does this change do, in a sentence or two? -->

## Why

<!-- What problem does this solve, or what does it enable? Link an issue if there is one. -->

## Checklist

- [ ] Every commit has a `Signed-off-by:` trailer (`git commit -s`) — see [CONTRIBUTING.md](../CONTRIBUTING.md#developer-certificate-of-origin-dco)
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-features --all-targets -- -D warnings` passes
- [ ] `cargo test --all-features` (or `cargo nextest run --all-features`) passes
- [ ] New behavior has test coverage; bug fixes include a regression test
- [ ] `cargo deny check` passes if dependencies changed
