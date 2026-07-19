#!/usr/bin/env bash
# Changelog fragment gate.
#
# Runs in pre-commit and CI. Fails unless at least one valid changelog.d fragment
# is staged (or the branch contains fragments when compared to main). Bypass with
# CHANGELOG_SKIP=1 for genuinely non-user-facing changes.

set -euo pipefail

if [ -n "${CHANGELOG_SKIP:-}" ]; then
  echo "Changelog check skipped (CHANGELOG_SKIP is set)."
  exit 0
fi

cd "$(git rev-parse --show-toplevel)"

TOWNCRIER="towncrier"
if ! command -v towncrier >/dev/null 2>&1; then
  if command -v uvx >/dev/null 2>&1; then
    TOWNCRIER="uvx --from towncrier towncrier"
  else
    echo "Error: 'towncrier' is not installed and 'uvx' is not available." >&2
    echo "Install with: uv tool install towncrier" >&2
    exit 1
  fi
fi

VALID_TYPES="added|changed|deprecated|removed|fixed|security"

# Validate staged changelog.d fragments.
STAGED_FRAGMENTS=$(git diff --cached --name-only --diff-filter=ACM -- 'changelog.d/*.md' || true)
BROKEN=""

for f in $STAGED_FRAGMENTS; do
  base=$(basename "$f")
  if [ "$base" = "README.md" ] || [ "$base" = ".gitkeep" ]; then
    continue
  fi
  if ! echo "$base" | grep -qE "^[A-Za-z0-9+_.-]+\.($VALID_TYPES)\.md$"; then
    echo "Error: changelog fragment '$f' does not match '<name>.<type>.md'" >&2
    echo "Valid types: $VALID_TYPES" >&2
    BROKEN=1
    continue
  fi
  if [ -f "$f" ] && head -n 1 "$f" | grep -qE "^- "; then
    echo "Error: changelog fragment '$f' starts with '- '. Fragments are rendered as bullets by towncrier; remove the leading '- '." >&2
    BROKEN=1
  fi
done

if [ -n "$BROKEN" ]; then
  exit 1
fi

# Determine the best base ref and run towncrier's branch check.
BASE_REF=""
for ref in origin/main main; do
  if git rev-parse --verify "$ref" >/dev/null 2>&1; then
    BASE_REF="$ref"
    break
  fi
done

if [ -z "$BASE_REF" ]; then
  # No base branch available; perform a best-effort staged-file check.
  STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM)
  if [ -z "$STAGED_FILES" ]; then
    exit 0
  fi

  # Exempt paths that should not require a changelog fragment.
  EXEMPT=$(echo "$STAGED_FILES" | grep -vE '^(CHANGELOG\.md|changelog\.d/|docs/|\.github/|\.githooks/|tools/check-changelog\.sh|.*\.md|.*\.nix|\.envrc|\.gitignore|\.gitattributes|rust-toolchain\.toml|rustfmt\.toml|clippy\.toml|deny\.toml|\.config/|\.mise\.toml|supply-chain/|fuzz/|Cargo\.lock)$' || true)
  if [ -z "$EXEMPT" ]; then
    exit 0
  fi

  FRAGMENTS=$(echo "$STAGED_FILES" | grep -E "^changelog\.d/[^/]+\.($VALID_TYPES)\.md$" || true)
  if [ -z "$FRAGMENTS" ]; then
    echo "Error: non-exempt files are staged but no changelog.d fragment was added." >&2
    echo "Add a fragment, or set CHANGELOG_SKIP=1 to bypass for non-user-facing changes." >&2
    exit 1
  fi
  exit 0
fi

$TOWNCRIER check --compare-with "$BASE_REF" --staged
