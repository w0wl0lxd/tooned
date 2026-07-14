//! T078c: dependency-boundary guard (constitution Principle III,
//! dependency-minimal core). Asserts `cargo tree -p tooned-core` contains
//! none of `rusqlite`/`ignore`/`walkdir` for the crate's default (non-dev)
//! build -- those belong in `tooned-index` only, invoked on-demand, never
//! on `tooned-core`'s hot hook path. Previously guaranteed only by manual
//! scaffold review; this makes a future accidental regression (e.g. an
//! errant `tooned-index` path dependency creeping into `tooned-core`)
//! fail CI automatically instead.
//!
//! Scoped to `-e normal` (excludes dev-dependencies) deliberately:
//! `criterion` (a dev-dependency, used by `benches/`) transitively pulls in
//! `walkdir` for its own internal use, which never ships in an embedded
//! `tooned-core` library and is out of scope for this guard.

use std::process::Command;

const BANNED_CRATES: &[&str] = &["rusqlite", "ignore", "walkdir"];

#[test]
fn tooned_core_default_build_has_no_heavyweight_index_only_deps() {
    let output = Command::new(env!("CARGO"))
        .args(["tree", "-p", "tooned-core", "-e", "normal", "--prefix", "none"])
        .output();
    let output = match output {
        Ok(output) => output,
        Err(err) => panic!("failed to run `cargo tree`: {err}"),
    };
    assert!(
        output.status.success(),
        "`cargo tree -p tooned-core` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = match String::from_utf8(output.stdout) {
        Ok(tree) => tree,
        Err(err) => panic!("cargo tree output was not valid UTF-8: {err}"),
    };

    for line in tree.lines() {
        // `cargo tree --prefix none` output is one crate per line, e.g.
        // `rusqlite v0.32.1`; the crate name is the token before the first
        // space.
        let Some(crate_name) = line.split_whitespace().next() else { continue };
        for banned in BANNED_CRATES {
            assert!(
                crate_name != *banned,
                "found `{crate_name}` in tooned-core's default (non-dev) dependency tree -- \
                 heavyweight/on-demand-only dependencies belong in tooned-index only \
                 (constitution Principle III); full tree:\n{tree}"
            );
        }
    }
}
