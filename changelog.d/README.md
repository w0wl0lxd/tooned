# Changelog fragments (`towncrier`)

Each user-facing PR adds a fragment here. Fragments compile into `CHANGELOG.md` at release time.

Filename: `<issue-or-pr>.<type>.md` or `+<slug>.<type>.md` (for untracked changes). Types: `security`, `removed`, `deprecated`, `added`, `changed`, `fixed`.

Content: a single Markdown bullet (no leading `- `; `towncrier` adds it). Example (`+xml-support.added.md`):

```markdown
XML input support (detect + parse + adaptive TOON conversion). Streaming parser with `proptest` coverage for round-trip fidelity.
```

Enforcement: `.githooks/pre-commit`, `.github/workflows/ci.yml`, `just changelog-check`. Skip with `CHANGELOG_SKIP=1` for non-user-facing changes.
