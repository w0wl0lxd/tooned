# Changelog fragments

This directory contains `towncrier` changelog fragments. Each PR that makes a
user-facing change should add at least one fragment here. Fragments are compiled
into `CHANGELOG.md` when a release is cut.

## Filename convention

```text
<optional-issue-or-pr>.<type>.md
+<slug>.<type>.md         # for changes without a tracked issue/PR
```

Valid `<type>` values: `security`, `removed`, `deprecated`, `added`, `changed`,
`fixed`.

## Content

Fragment files contain a single Markdown bullet entry without a leading `- `.
`towncrier` adds the bullet prefix when it assembles `CHANGELOG.md`.

Good example (`+xml-support.added.md`):

```markdown
XML input support (detect + parse + adaptive TOON conversion). The sniffer is
conservative, the `quick-xml` parser uses streaming events, and `proptest`
covers XML round-trip fidelity and no-panic behavior.
```

## Enforcement

- Pre-commit: `.githooks/pre-commit` runs `tools/check-changelog.sh`.
- CI: `.github/workflows/ci.yml` runs `tools/check-changelog.sh`.
- Local: `just changelog-check` or `towncrier check --compare-with main --staged`.

To bypass the check for a genuinely non-user-facing change (e.g., typo in an
internal comment), set `CHANGELOG_SKIP=1` when committing:

```bash
CHANGELOG_SKIP=1 git commit -m "chore: fix typo"
```
