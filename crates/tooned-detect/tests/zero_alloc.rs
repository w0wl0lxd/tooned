// SPDX-License-Identifier: AGPL-3.0-only

//! Zero-allocation regression test for `tooned_detect::detect`.
//!
//! This lives in its own integration-test binary so that `heapster` only sees
//! the allocations performed by this test. All cases run in a single `#[test]`
//! so that `cargo test` (which runs tests in a single binary concurrently)
//! cannot attribute another test's thread allocations to this one.

#![forbid(unsafe_code)]

use std::alloc::System;

use tooned_detect::detect;
use tooned_types::DocType;

#[global_allocator]
static GLOBAL: heapster::Heapster<System> = heapster::Heapster::new(System);

#[test]
fn detect_is_zero_allocation_on_representative_inputs() {
    if std::env::var_os("CARGO_LLVM_COV").is_some() {
        return;
    }

    let hint = GLOBAL.measure(|| detect(b"not json", Some(DocType::Json)));
    assert_eq!(hint.1.alloc_count, 0, "hint path must not perform any heap allocations");
    assert_eq!(hint.1.alloc_sum, 0, "hint path must not allocate any heap bytes");

    let cases: &[&[u8]] = &[
        br#"{"a": 1, "b": [1,2,3]}"#,
        b"[1, 2, 3]",
        b"{\"a\":1}\n{\"a\":2}\n",
        b"---\nname: alice\nage: 30\n",
        b"[server]\nhost = \"localhost\"\nport = 8080\n",
        b"name,age,active\nalice,30,true\nbob,25,false\n",
        b"name\tage\tactive\nalice\t30\ttrue\nbob\t25\tfalse\n",
        b"this is just some prose without any structure at all",
        b"",
    ];
    for input in cases {
        let (_, diff) = GLOBAL.measure(|| detect(input, None));
        assert_eq!(diff.alloc_count, 0, "detect({input:?}) must not perform any heap allocations");
        assert_eq!(diff.alloc_sum, 0, "detect({input:?}) must not allocate any heap bytes");
    }
}
