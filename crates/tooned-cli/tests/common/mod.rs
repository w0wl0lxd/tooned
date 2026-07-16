// SPDX-License-Identifier: AGPL-3.0-only

//! Shared test-only helpers for `tooned-cli`'s integration tests.
//! `mod common;` is duplicated per integration-test binary (a `tests/`
//! convention already used elsewhere in this workspace).
#![allow(dead_code)]

pub mod mcp_client;
pub mod xml;
