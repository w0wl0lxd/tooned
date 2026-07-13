# Data Model: Adaptive TOON Conversion for AI Agent Tool-Call Context

Derived from spec.md's Key Entities section, made concrete for the Rust workspace.

## Conversion Decision (`tooned-core`)

In-memory only — never persisted by `tooned-core` itself (persistence, when it
happens, is `tooned-index`'s `conversions` table below).

```rust
pub struct ConversionOptions {
    pub margin_pct: f64,          // default 2.0
    pub max_input_bytes: usize,   // default 2 * 1024 * 1024
    pub format_hint: Option<DocType>,
    pub precise_tokens: bool,     // default false; opt-in, never the hot-loop default
}

pub enum DocType { Json, NdJson, Yaml, Toml, Csv, Tsv }

pub enum Conversion {
    Toon { text: String, report: ConversionReport },
    Passthrough { bytes: Vec<u8>, reason: PassthroughReason },
}

pub struct ConversionReport {
    pub doc_type: DocType,
    pub shape: ShapeClass,
    pub json_bytes: usize,
    pub toon_bytes: usize,
    pub savings_pct: f64,
}

pub enum PassthroughReason {
    NotStructuredData,
    ParseFailed,
    InputTooLarge,
    NotSmallerEnough { json_bytes: usize, toon_bytes: usize },
    RoundTripMismatch,   // conversion computed but failed the fidelity check (FR-008); never surfaced as Toon
}

pub enum ToonedError {
    InputTooLarge,
    // internal-only variants for propagating parse errors before they're
    // downgraded to PassthroughReason at the maybe_tooned boundary
}
```

**Validation rules** (from FR-004, FR-006, FR-008):
- `Conversion::Toon` is only ever constructed when `toon_bytes < json_bytes * (1.0 - margin_pct/100.0)` AND the round-trip check (decode the TOON text, re-encode to compact JSON, compare) succeeds.
- `maybe_tooned` never returns `Err` for malformed/oversized input — those map to `Conversion::Passthrough`, not an error. `ToonedError` is reserved for genuine caller-misuse (e.g., invalid `ConversionOptions`), not payload-driven failure.

## Payload Shape Profile (`tooned-core`)

```rust
pub enum ShapeClass {
    UniformArrayOfObjects { uniformity_pct: f64, sampled: usize },
    Irregular,
    Scalar,
}
```

**Rules** (from spec.md's shape classification and the plan's `K=64` sampling):
- Sample up to `K = 64` elements of a top-level (or, for CSV/TSV, row-derived) array.
- Compute each sampled element's key-signature (sorted key set for objects).
- `uniformity_pct` = fraction of sampled elements sharing the most common key-signature.
- `UniformArrayOfObjects` requires `uniformity_pct >= 0.9`; anything else is `Irregular`. A non-array top-level value is `Scalar`.
- `ShapeClass` is descriptive/diagnostic (surfaced via `tooned check`) — it does NOT gate the conversion decision on its own; the byte-size comparison (Conversion Decision, above) is the sole gate, per spec.md's explicit note that the adaptive decision "is applied regardless of shape class."

## Project Index (`tooned-index`, SQLite at `.tooned/index.db`)

```sql
CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
); -- schema_version, created_at

CREATE TABLE files (
    path         TEXT PRIMARY KEY,   -- relative to project root
    size_bytes   INTEGER NOT NULL,
    mtime_unix   INTEGER NOT NULL,
    content_hash TEXT NOT NULL,      -- blake3 hex digest
    doc_type     TEXT,               -- NULL if not a recognized doctype
    scanned_at   INTEGER NOT NULL
);

CREATE TABLE shapes (
    path            TEXT NOT NULL,
    json_pointer     TEXT NOT NULL,   -- '' for the document root, else JSON Pointer to a nested array
    shape_class      TEXT NOT NULL,   -- 'uniform' | 'irregular' | 'scalar'
    uniformity_pct   REAL,
    sampled_count    INTEGER,
    PRIMARY KEY (path, json_pointer),
    FOREIGN KEY (path) REFERENCES files(path) ON DELETE CASCADE
);

CREATE TABLE conversions (
    path            TEXT NOT NULL,
    json_pointer     TEXT NOT NULL,
    json_bytes       INTEGER NOT NULL,
    toon_bytes       INTEGER NOT NULL,
    savings_pct      REAL NOT NULL,
    cached_toon_text TEXT,            -- NULL unless below the cache-size cutoff
    computed_at      INTEGER NOT NULL,
    PRIMARY KEY (path, json_pointer),
    FOREIGN KEY (path) REFERENCES files(path) ON DELETE CASCADE
);
```

**Rules** (from FR-020, FR-021, FR-022, and research.md #5):
- `files.content_hash` (blake3) plus `mtime_unix` together drive `sync`'s incremental
  decision: if `mtime_unix` is unchanged since the last scan, skip re-hashing entirely;
  if `mtime_unix` changed but `content_hash` is unchanged (e.g., touch without edit),
  skip re-classification but update `mtime_unix`.
- `sync` deletes `files` rows (cascading to `shapes`/`conversions`) for paths no longer
  present under the scanned root.
- On the index's first creation for a project, `tooned-index` appends `.tooned/` to
  that project's `.gitignore` if not already covered (FR-020, research.md #5).
- `stats --top N` is a read-only query: `SELECT ... FROM conversions ORDER BY savings_pct DESC LIMIT N` joined back to `files` for the path.

## Integration Installation Record (`tooned-cli`, lives inside each agent's own config file — not a tooned-owned store)

Conceptual model only (no tooned-owned schema; enforced by installer logic, not storage):

```rust
pub enum AgentTarget { ClaudeCode { scope: Scope }, Codex }
pub enum Scope { User, Project }

pub struct HookEntry {
    pub matcher: String,      // e.g. "Bash|Read|Grep|WebFetch|^mcp__"
    pub command: String,      // absolute path to the tooned binary + hook subcommand
}
```

**Rules** (from FR-016, FR-017, FR-018, FR-019, clarification's PATH-check answer):
- Identity for idempotent install/uninstall is the exact `command` string (FR-016): the
  installer searches the target's existing hook array for an entry whose `command`
  matches before appending, never appending a duplicate.
- Install MUST resolve the `tooned` binary on `PATH` (or use the invoking binary's own
  absolute path) and abort with a clear error before writing any entry if that
  resolution fails.
- Uninstall removes only entries whose `command` matches tooned's own; all other array
  elements (including another tool's, e.g. rtk's) are left byte-for-byte as they were.
- `tooned hook doctor` reads (never writes) the target config and reports every hook
  entry found — tooned's own and others' — by `command`/`matcher`, for diagnosis.

## Agent Integration Surface (conceptual, cross-cutting)

Not a data type — an enum-of-entrypoints that all funnel into the same
`tooned_core::maybe_tooned` call, per constitution Principle V ("no parallel
implementation"):

1. Standalone CLI (`tooned convert`/`check`/`pipe`/`wrap`)
2. Claude Code `PostToolUse` hook subprocess
3. Codex CLI `PostToolUse` hook subprocess
4. MCP server tool calls (`tooned_convert`, `tooned_detect`, `tooned_decode`)
