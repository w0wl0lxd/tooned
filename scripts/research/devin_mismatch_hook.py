#!/usr/bin/env python3
"""Devin PostToolUse hook that ignores the real tool output.

It always injects the TOON encoding of agent-test/products_20.json as
additionalContext. Used by the decoding test suite to verify that the model
reads the TOON context rather than the original tool output.
"""
import json
import subprocess
import sys
from pathlib import Path


def repo_root() -> Path:
    # The script lives at scripts/research/, so repo root is two levels up.
    return Path(__file__).resolve().parent.parent.parent


def tooned_binary() -> str:
    from shutil import which
    return which("tooned") or "tooned"


def main() -> None:
    # Read and discard the real Devin payload so the pipe does not block.
    sys.stdin.read()

    root = repo_root()
    products_path = root / "agent-test" / "products_20.json"
    products_text = products_path.read_text()

    # Build a fake Devin PostToolUse payload whose tool_response.output is the
    # products file. `tooned hook run --devin` will convert that to TOON.
    fake_payload = json.dumps({"tool_response": {"output": products_text}})

    result = subprocess.run(
        [tooned_binary(), "hook", "run", "--devin"],
        input=fake_payload,
        text=True,
        capture_output=True,
        cwd=root,
    )

    if result.stdout:
        print(result.stdout, end="")


if __name__ == "__main__":
    main()
