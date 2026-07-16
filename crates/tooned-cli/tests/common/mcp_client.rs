//! Minimal blocking JSON-RPC client for driving `tooned mcp serve` over
//! real stdio pipes in integration tests (T073/T074). Deliberately hand-
//! rolled rather than depending on an `rmcp` client transport in
//! `tooned-cli`'s dev-dependencies: this exercises the actual wire
//! protocol a real MCP client would speak, the same newline-delimited
//! JSON-RPC framing `rmcp`'s own stdio transport tests use.
//!
//! `mod common;` is duplicated per integration-test binary (a `tests/`
//! convention already used elsewhere in this crate).
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

// Generous on purpose: under a fully-parallel `cargo nextest run` (many
// `tooned mcp serve` subprocesses spawned concurrently, contending for
// CPU), an individual response has been observed taking >10s despite the
// same test completing in ~1s in isolation -- this is subprocess-startup/
// scheduling contention, not a real hang, so the bound needs enough
// headroom to not flake under normal CI parallelism while still catching a
// genuine stuck server.
const RESPONSE_TIMEOUT: Duration = Duration::from_mins(1);

pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    rx: mpsc::Receiver<String>,
    next_id: u64,
}

impl McpClient {
    /// Spawns `tooned mcp serve` and completes the MCP `initialize`
    /// handshake before returning, so every test can go straight to
    /// `call_tool`.
    #[allow(clippy::expect_used)]
    pub fn spawn() -> Self {
        let bin = assert_cmd::cargo::cargo_bin("tooned");
        let mut child = Command::new(bin)
            .args(["mcp", "serve"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn `tooned mcp serve`");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");

        // Reader thread: the stdio transport's response ordering isn't
        // guaranteed to line up 1:1 with a purely synchronous request/
        // response loop (e.g. notifications), so responses are buffered
        // off a background thread onto a channel a blocking test can poll.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx.send(line.trim_end().to_string()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let mut client = Self { child, stdin, rx, next_id: 1 };
        client.initialize();
        client
    }

    #[allow(clippy::expect_used)]
    fn send(&mut self, message: &Value) {
        let mut line = serde_json::to_string(message).expect("serialize JSON-RPC message");
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).expect("write to child stdin");
        self.stdin.flush().expect("flush child stdin");
    }

    #[allow(clippy::expect_used)]
    fn recv(&self) -> Value {
        let line = self
            .rx
            .recv_timeout(RESPONSE_TIMEOUT)
            .expect("`tooned mcp serve` response within timeout");
        serde_json::from_str(&line).expect("valid JSON-RPC response line")
    }

    /// Reads responses until one carries the given request `id` (skipping
    /// any interleaved notification without an `id`).
    fn recv_for_id(&self, id: u64) -> Value {
        loop {
            let value = self.recv();
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                return value;
            }
        }
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn initialize(&mut self) {
        let id = self.next_id();
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "tooned-mcp-tools-test", "version": "0.0.0" }
            }
        }));
        let _ = self.recv_for_id(id);
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
    }

    /// Calls `tools/call` for `name` with `arguments` and returns the full
    /// JSON-RPC response (`result` on success, `error` on a protocol-level
    /// failure -- distinct from a tool-level error, which still comes back
    /// as a `result` with `isError: true`).
    pub fn call_tool(&mut self, name: &str, arguments: &Value) -> Value {
        let id = self.next_id();
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }));
        self.recv_for_id(id)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
