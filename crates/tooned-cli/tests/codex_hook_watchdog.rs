// SPDX-License-Identifier: AGPL-3.0-only

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]


//! Integration test (T027b): `tooned hook run --codex` must exit within its
//! internal watchdog bound even when the worker computing the conversion
//! stalls, since Codex CLI does not blanket-guarantee fail-open behavior for
//! a hung hook process (contracts/codex-hook.md).
//!
//! `TOONED_CODEX_TEST_STALL_MS` is a deliberate, documented test-only seam
//! (see `src/hooks/codex.rs`) used here to simulate a naive/stalled
//! implementation without depending on `tooned-core` (which is designed to
//! be fast and therefore hard to stall legitimately).

use std::time::{Duration, Instant};

use assert_cmd::Command;

#[test]
fn codex_hook_run_exits_via_watchdog_well_before_a_stalled_worker_finishes() {
    let stall_ms: u64 = 10_000;
    let stdin = serde_json::json!({
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {},
        "tool_response": "just some prose, nothing structured here",
    })
    .to_string();

    let start = Instant::now();
    Command::cargo_bin("tooned")
        .expect("binary exists")
        .args(["hook", "run", "--codex"])
        .env("TOONED_CODEX_TEST_STALL_MS", stall_ms.to_string())
        .write_stdin(stdin)
        .assert()
        .success();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(stall_ms),
        "hook run --codex must exit via its internal watchdog well before the \
         stalled worker thread would finish on its own (stalled {stall_ms}ms, \
         actually took {elapsed:?})"
    );
}
