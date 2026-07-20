// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! Content tests for the TOON evidence/decoding docs shipped under
//! `docs/agents/` and linked from the root `README.md`.
//!
//! This PR rewrote several docs, deleted three that were folded into their
//! neighbors, and reworded a specific README claim. Those docs make
//! checkable assertions (row counts that must match summary sentences,
//! cross-links between docs, an embedded JSON example, files that were
//! intentionally removed). This suite guards against silent drift: a stale
//! link, a table edited without updating its summary count, or a
//! reintroduced reference to a file that no longer exists.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns the workspace root (two levels up from `crates/tooned-cli`).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/tooned-cli has a parent directory")
        .parent()
        .expect("crates/ has a parent directory")
        .to_path_buf()
}

/// Reads a doc file addressed relative to the repo root.
fn read_doc(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Collapses all whitespace runs (including line breaks from prose that
/// wraps at ~80 columns) to a single space, so substring checks on wrapped
/// paragraphs don't depend on exact line-wrap positions.
fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extracts the target of every inline markdown link `[text](target)` in
/// `content`, in order of appearance. Does not handle nested parentheses
/// inside the link target (none of the docs under test use them).
fn extract_markdown_link_targets(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = content;
    while let Some(open) = rest.find("](") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find(')') else {
            break;
        };
        targets.push(after_open[..close].to_string());
        rest = &after_open[close + 1..];
    }
    targets
}

/// Resolves a markdown link target relative to the directory containing the
/// markdown file that contains it, stripping any `#fragment` anchor.
/// Returns `None` for links this test does not check (external URLs,
/// `mailto:`, pure same-page anchors).
fn resolve_relative_link(markdown_file: &Path, target: &str) -> Option<PathBuf> {
    // `str::split` always yields at least one item, even with no delimiter present.
    let target = target.split('#').next().expect("split always yields at least one item");
    if target.is_empty()
        || target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
    {
        return None;
    }
    let dir = markdown_file.parent().expect("markdown file has a parent dir");
    Some(dir.join(target))
}

/// Every relative link inside a markdown file must point at a file that
/// actually exists on disk.
fn assert_relative_links_resolve(relative_doc_path: &str) {
    let content = read_doc(relative_doc_path);
    let doc_path = repo_root().join(relative_doc_path);
    let mut checked_any_relative_link = false;
    for target in extract_markdown_link_targets(&content) {
        if let Some(resolved) = resolve_relative_link(&doc_path, &target) {
            checked_any_relative_link = true;
            assert!(
                resolved.exists(),
                "{relative_doc_path} links to `{target}`, which resolves to {} \
                 and does not exist",
                resolved.display()
            );
        }
    }
    assert!(
        checked_any_relative_link,
        "{relative_doc_path} contains no relative markdown links; update this \
         test if that is intentional"
    );
}

#[test]
fn readme_relative_links_resolve() {
    assert_relative_links_resolve("README.md");
}

#[test]
fn toon_decoding_relative_links_resolve() {
    assert_relative_links_resolve("docs/agents/toon-decoding.md");
}

#[test]
fn toon_evidence_relative_links_resolve() {
    assert_relative_links_resolve("docs/agents/toon-evidence.md");
}

#[test]
fn toon_example_relative_links_resolve() {
    assert_relative_links_resolve("docs/agents/toon-example.md");
}

#[test]
fn toon_context_proof_relative_links_resolve() {
    assert_relative_links_resolve("docs/agents/toon-context-proof.md");
}

#[test]
fn toon_hook_flow_relative_links_resolve() {
    assert_relative_links_resolve("docs/agents/toon-hook-flow.md");
}

#[test]
fn toon_format_research_relative_links_resolve() {
    assert_relative_links_resolve("docs/agents/research/toon-format-research.md");
}

/// Docs removed by this change; folded into `toon-evidence.md` and
/// `toon-format-research.md`. They must not be silently reintroduced.
const DELETED_DOCS: &[&str] = &[
    "docs/agents/toon-comprehension-evidence.md",
    "docs/agents/toon-model-decoding.md",
    "docs/agents/research/toon-decoding-test-suite.md",
];

/// Every doc changed by this PR that is still expected to exist afterward.
const SURVIVING_AGENT_DOCS: &[&str] = &[
    "README.md",
    "docs/agents/toon-decoding.md",
    "docs/agents/toon-evidence.md",
    "docs/agents/toon-example.md",
    "docs/agents/toon-context-proof.md",
    "docs/agents/toon-hook-flow.md",
    "docs/agents/research/toon-format-research.md",
];

#[test]
fn deleted_docs_no_longer_exist() {
    for relative in DELETED_DOCS {
        let path = repo_root().join(relative);
        assert!(
            !path.exists(),
            "{relative} was removed by this change and must not be reintroduced"
        );
    }
}

#[test]
fn no_surviving_doc_references_a_deleted_file() {
    for doc in SURVIVING_AGENT_DOCS {
        let content = read_doc(doc);
        for deleted in DELETED_DOCS {
            let deleted_filename = Path::new(deleted)
                .file_name()
                .and_then(|f| f.to_str())
                .expect("deleted doc path has a file name");
            assert!(
                !content.contains(deleted_filename),
                "{doc} still references deleted file `{deleted_filename}`"
            );
        }
    }
}

/// The README must not claim a direct (non-mismatch) `read` proves the model
/// reasoned entirely over a TOON `additionalContext`, because `tooned` does not
/// emit `additionalContext`. It should still acknowledge that the model can read
/// the TOON result as if it were the original JSON.
#[test]
fn readme_toon_summary_describes_toon_without_additionalcontext() {
    let readme = normalize_whitespace(&read_doc("README.md"));
    assert!(
        readme.contains(
            "model still read and reasoned about the data as if it were the original JSON"
        ),
        "README.md should describe the model reasoning over the TOON result"
    );
    assert!(
        !readme.contains("reasoning entirely over the TOON `additionalContext`"),
        "README.md must not reintroduce the retracted claim that the model \
         reasoned entirely over the TOON additionalContext for a direct read"
    );
}

#[test]
fn readme_links_to_toon_example_and_toon_evidence() {
    let readme = read_doc("README.md");
    assert!(readme.contains("(docs/agents/toon-example.md)"));
    assert!(readme.contains("(docs/agents/toon-evidence.md)"));
}

/// Returns the slice of `content` starting right after `heading` and ending
/// right before the next level-2 heading (or end of file).
fn section<'a>(content: &'a str, heading: &str) -> &'a str {
    let start =
        content.find(heading).unwrap_or_else(|| panic!("heading `{heading}` not found in doc"));
    let after_heading = &content[start + heading.len()..];
    let end = if let Some(pos) = after_heading.find("\n## ") { pos } else { after_heading.len() };
    &after_heading[..end]
}

/// Counts table data rows, whether rendered as a markdown table or inside a
/// fenced code block. Excludes header and separator rows.
fn count_table_data_rows(section: &str) -> usize {
    let mut in_code = false;
    section
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed == "```" {
                in_code = !in_code;
                return false;
            }
            if in_code {
                trimmed.contains('|')
                    && !trimmed.starts_with("Scenario")
                    && !trimmed.starts_with("---")
            } else {
                trimmed.starts_with('|')
                    && !trimmed.starts_with("|---")
                    && !trimmed.starts_with("| Scenario")
            }
        })
        .count()
}

#[test]
fn toon_evidence_full_savings_table_matches_summary() {
    let content = read_doc("docs/agents/toon-evidence.md");
    let table_section = section(&content, "## Full savings calculation");
    let row_count = count_table_data_rows(table_section);
    assert_eq!(row_count, 6, "expected 6 scenario rows in the full savings table");
    assert!(
        table_section.contains("Overall savings: 59.6%"),
        "summary sentence must match the computed overall savings"
    );
}

/// Returns `true` if every fenced ```` ```mermaid ```` block in `content` has
/// a matching closing fence.
fn mermaid_fences_are_balanced(content: &str) -> (bool, usize) {
    let mut in_mermaid = false;
    let mut mermaid_blocks = 0usize;
    for line in content.lines() {
        let trimmed = line.trim();
        if in_mermaid {
            if trimmed == "```" {
                in_mermaid = false;
            }
        } else if trimmed.starts_with("```mermaid") {
            in_mermaid = true;
            mermaid_blocks += 1;
        }
    }
    (!in_mermaid, mermaid_blocks)
}

#[test]
fn mermaid_fences_are_balanced_detects_an_unterminated_block() {
    let unterminated = "# doc\n\n```mermaid\nsequenceDiagram\n  A->>B: hi\n";
    let (balanced, blocks) = mermaid_fences_are_balanced(unterminated);
    assert!(!balanced, "an unterminated ```mermaid fence must be detected");
    assert_eq!(blocks, 1);
}

/// Extracts the content of the first fenced code block tagged `lang`
/// (e.g. "json"), excluding the fences themselves.
fn first_fenced_block<'a>(content: &'a str, lang: &str) -> &'a str {
    let marker = format!("```{lang}");
    let start =
        content.find(&marker).unwrap_or_else(|| panic!("doc has no ```{lang} fenced block"));
    let after = &content[start + marker.len()..];
    let end = after.find("```").unwrap_or_else(|| panic!("```{lang} fence is never closed"));
    &after[..end]
}

/// The example hook output embedded in `toon-context-proof.md` is presented
/// as the literal JSON `tooned` emits on `stdout` for agents that replace the
/// tool result. It must actually be valid JSON with the documented
/// `hookSpecificOutput.updatedToolOutput` shape, since agents parse this
/// contract directly.
fn json_field<'a>(value: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    value.get(key).unwrap_or_else(|| panic!("expected JSON field `{key}` in {value}"))
}

#[test]
fn toon_context_proof_example_hook_output_is_valid_json() {
    let content = read_doc("docs/agents/toon-context-proof.md");
    let json_text = first_fenced_block(&content, "json");
    let value: serde_json::Value = serde_json::from_str(json_text)
        .unwrap_or_else(|e| panic!("example hook output is not valid JSON: {e}\n{json_text}"));

    let hook_output = json_field(&value, "hookSpecificOutput");
    assert_eq!(json_field(hook_output, "hookEventName"), "PostToolUse");
    assert!(
        json_field(hook_output, "updatedToolOutput").is_string(),
        "updatedToolOutput must be a string containing the TOON payload"
    );
}

/// The protocol table in `toon-hook-flow.md` is the source of truth other
/// docs point to; pin its rows so a future edit can't silently change which
/// agents replace the tool result (`updatedToolOutput` / `reason` feedback)
/// versus pass through without emitting `additionalContext`.
#[test]
fn toon_hook_flow_protocol_table_maps_agents_to_expected_fields() {
    let content = read_doc("docs/agents/toon-hook-flow.md");
    let table_section = section(
        &content,
        "| Agent | Tool result field | Replacement mechanism | Original output |",
    );

    let claude_row = table_section
        .lines()
        .find(|l| l.contains("Claude Code, OpenCode, Kilo, Pi"))
        .expect("table must contain a row for Claude Code / OpenCode / Kilo / Pi");
    assert!(
        claude_row.contains("`hookSpecificOutput`") && claude_row.contains("`updatedToolOutput`"),
        "Claude Code/OpenCode/Kilo/Pi must surface TOON via hookSpecificOutput.updatedToolOutput"
    );

    let codex_row = table_section
        .lines()
        .find(|l| l.contains("Codex") && !l.contains("Devin") && !l.contains("Droid"))
        .expect("table must contain a row for Codex");
    assert!(
        codex_row.contains("`continue: false`") && codex_row.contains("`reason`"),
        "Codex must surface TOON via continue:false + reason feedback"
    );

    let devin_droid_row = table_section
        .lines()
        .find(|l| l.contains("Devin") || l.contains("Droid"))
        .expect("table must contain a row for Devin / Droid");
    assert!(
        devin_droid_row.contains("passes through") || devin_droid_row.contains("(none"),
        "Devin/Droid must pass through without emitting additionalContext"
    );
    assert!(
        !devin_droid_row.contains("`hookSpecificOutput.additionalContext`"),
        "Devin/Droid must not map to hookSpecificOutput.additionalContext"
    );
}

#[test]
fn extract_markdown_link_targets_handles_multiple_links_on_one_line() {
    let content = "See [a](one.md) and also [b](two.md#anchor) for more.";
    let targets = extract_markdown_link_targets(content);
    assert_eq!(targets, vec!["one.md".to_string(), "two.md#anchor".to_string()]);
}

#[test]
fn extract_markdown_link_targets_ignores_bracketed_text_without_a_link() {
    let content = "This uses [square brackets] but is not a link, and has no parens after.";
    let targets = extract_markdown_link_targets(content);
    assert!(targets.is_empty());
}

#[test]
fn resolve_relative_link_skips_external_and_anchor_only_targets() {
    let doc = Path::new("/repo/docs/agents/toon-evidence.md");
    assert!(resolve_relative_link(doc, "https://example.com/spec").is_none());
    assert!(resolve_relative_link(doc, "mailto:someone@example.com").is_none());
    assert!(resolve_relative_link(doc, "#some-anchor").is_none());
}

#[test]
fn resolve_relative_link_strips_fragment_and_resolves_against_parent_dir() {
    let doc = Path::new("/repo/docs/agents/toon-evidence.md");
    let resolved = resolve_relative_link(doc, "toon-context-proof.md#proof").unwrap();
    assert_eq!(resolved, Path::new("/repo/docs/agents/toon-context-proof.md"));
}

#[test]
fn resolve_relative_link_handles_parent_directory_traversal() {
    let doc = Path::new("/repo/docs/agents/research/toon-format-research.md");
    let resolved = resolve_relative_link(doc, "../toon-evidence.md").unwrap();
    // Not canonicalized, but must join to the textually correct path, exactly
    // like `research/toon-format-research.md` links back up to
    // `docs/agents/toon-evidence.md` in the real repo.
    assert_eq!(resolved, Path::new("/repo/docs/agents/research/../toon-evidence.md"));
    assert_eq!(resolved.file_name(), Some(std::ffi::OsStr::new("toon-evidence.md")));
}
