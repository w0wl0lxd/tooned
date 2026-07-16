//! T078b: network-call guard (FR-025 -- v1 has zero telemetry/external
//! calls). Asserts no network-capable crate appears in `cargo tree` for the
//! default (non-dev) build of any of the 3 workspace crates. Makes that a
//! regression-tested fact rather than a manual claim: if a future
//! dependency bump ever pulls in an HTTP/TLS client transitively (e.g. via
//! an `rmcp` feature flag flip), this fails loudly instead of silently
//! shipping a telemetry surface nobody asked for.
//!
//! Scoped to `-e normal` (excludes dev-dependencies) deliberately: a
//! dev-only dependency (e.g. `criterion` pulling in `walkdir` for its own
//! internal use) never ships in a built `tooned` binary or embedded
//! `tooned-core` library, so it's out of scope for "the default build".

use std::process::Command;

/// Crate name fragments that indicate real network capability (an HTTP/TLS
/// client or transport), not merely "the word contains an unrelated
/// substring" -- checked against `cargo tree`'s own crate-name column, one
/// crate per line, so a substring match here is unambiguous.
const NETWORK_CAPABLE_CRATES: &[&str] = &[
    "reqwest",
    "hyper",
    "hyper-util",
    "hyper-tls",
    "native-tls",
    "tokio-native-tls",
    "rustls",
    "tokio-rustls",
    "openssl",
    "openssl-sys",
    "curl",
    "curl-sys",
    "ureq",
    "isahc",
    "surf",
    "h2",
    "h3",
];

#[allow(clippy::expect_used)] // test-only helper in an integration-test binary
fn cargo_tree_normal_deps(package: &str) -> String {
    let output = Command::new(env!("CARGO"))
        .args(["tree", "-p", package, "-e", "normal", "--prefix", "none"])
        .output()
        .expect("run `cargo tree`");
    assert!(
        output.status.success(),
        "`cargo tree -p {package}` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("cargo tree output is valid UTF-8")
}

fn assert_no_network_capable_crate(package: &str) {
    let tree = cargo_tree_normal_deps(package);
    for line in tree.lines() {
        // `cargo tree --prefix none` output is one crate per line, e.g.
        // `reqwest v0.12.9`; the crate name is the token before the first
        // space.
        let Some(crate_name) = line.split_whitespace().next() else { continue };
        for banned in NETWORK_CAPABLE_CRATES {
            assert!(
                crate_name != *banned,
                "found network-capable crate `{crate_name}` in the default (non-dev) \
                 dependency tree of `{package}` -- v1 has zero telemetry/external network \
                 calls (FR-025); full tree:\n{tree}"
            );
        }
    }
}

#[test]
fn tooned_core_default_build_has_no_network_capable_crate() {
    assert_no_network_capable_crate("tooned-core");
}

#[test]
fn tooned_index_default_build_has_no_network_capable_crate() {
    assert_no_network_capable_crate("tooned-index");
}

#[test]
fn tooned_cli_default_build_has_no_network_capable_crate() {
    assert_no_network_capable_crate("tooned-cli");
}
