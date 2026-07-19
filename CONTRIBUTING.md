# Contributing to tooned

Thank you for your interest in contributing to tooned!

## Developer Certificate of Origin (DCO)

This project uses the [Developer Certificate of Origin](DCO.txt) (DCO) to ensure that all contributions can be legally incorporated into the project under its dual license (AGPL-3.0-only + Commercial).

### What is DCO?

The DCO is a lightweight way for contributors to certify that they have the right to submit their contribution. It was created by the Linux Foundation and is used by many open source projects.

### How to Sign Off

Every commit must include a `Signed-off-by` line in the commit message:

```
feat(core): Add CSV shape classification

Signed-off-by: Your Name <your.email@example.com>
```

You can add this automatically by using the `-s` flag with `git commit`:

```bash
git commit -s -m "feat(core): Add CSV shape classification"
```

### Why DCO?

Because tooned is dual-licensed (AGPL-3.0-only for open source, commercial for proprietary use), we need to ensure that all contributions can be distributed under both licenses. The DCO provides a simple, legally-binding way for contributors to certify they have this right.

## Contribution Guidelines

### What We Accept

- **Bug fixes**: Issues with existing functionality
- **Documentation**: Improvements to docs, examples, and comments
- **Tests**: Additional test coverage
- **Performance**: Optimizations with benchmarks

### What Requires Discussion First

Please open an issue or discussion before working on:

- **New features**: Let's align on design before implementation
- **Breaking changes**: Need to coordinate with release planning
- **Large refactors**: Discuss approach before investing time

### Changelog fragments

Every PR that changes user-visible behavior must add a fragment to
`changelog.d/` following the `towncrier` convention documented in
`changelog.d/README.md`. The pre-commit hook enforces this via
`tools/check-changelog.sh`. Install the changelog tool with:

```bash
uv tool install towncrier
```

Preview the rendered changelog with `just changelog-preview` or `towncrier
build --draft`. To cut a release, run `just changelog-build X.Y.Z`.

To bypass the check for a non-user-facing change (e.g., a typo fix in an
internal comment), set `CHANGELOG_SKIP=1` when committing.

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy --all-features -- -D warnings` to check for lints
- Add `changelog.d/<name>.<type>.md` fragments for user-facing changes
- Add tests for new functionality
- Update documentation as needed

### Local Git Hooks (optional)

`.githooks/` ships a `pre-commit` (runs `cargo fmt` + `cargo clippy` +
`tools/check-changelog.sh`), and a `commit-msg` (enforces the DCO trailer)
hook. Enable them once per clone:

```bash
git config core.hooksPath .githooks
```

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

[optional body]

Signed-off-by: Your Name <your.email@example.com>
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`

Scopes: `core`, `index`, `cli`, `hooks`, `mcp`, `docs`

### Pull Request Process

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes with DCO sign-off on all commits
4. Run tests: `cargo test --all-features`
5. Run lints: `cargo clippy --all-features -- -D warnings`
6. Submit a pull request

### Questions?

- Open a [GitHub Discussion](https://github.com/w0wl0lxd/tooned/discussions)
- Email: w0wl0lxd@tuta.com

## License

By contributing to tooned, you agree that your contributions will be licensed under the project's dual license (AGPL-3.0-only for open source distribution, with the option for commercial licensing). See [LICENSING.md](LICENSING.md) for details.
