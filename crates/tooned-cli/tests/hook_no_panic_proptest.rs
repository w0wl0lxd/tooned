// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Property test (T027): `tooned hook run` (both `--claude-code` and
//! `--codex`) never panics/crashes for adversarial or malformed stdin.
//! it must always exit 0 (constitution Principle I; contracts/*-hook.md).

use assert_cmd::Command;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn claude_code_hook_run_never_crashes_on_arbitrary_bytes(
        bytes in proptest::collection::vec(any::<u8>(), 0..2048)
    ) {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "run", "--claude-code"])
            .write_stdin(bytes)
            .assert()
            .code(0);
    }

    #[test]
    fn codex_hook_run_never_crashes_on_arbitrary_bytes(
        bytes in proptest::collection::vec(any::<u8>(), 0..2048)
    ) {
        Command::cargo_bin("tooned")
            .expect("binary exists")
            .args(["hook", "run", "--codex"])
            .write_stdin(bytes)
            .assert()
            .code(0);
    }
}
