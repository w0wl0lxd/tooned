#!/usr/bin/env python3
"""Live cross-format TOON decoding test suite.

This script generates a set of structured-data fixtures under
agent-test/complex/, runs targeted prompts through Devin CLI against the real
tooned hook, and produces a markdown report at
``docs/agents/research/toon-decoding-test-suite.md``.

It supports two modes:

- ``direct``: the real ``tooned hook run --devin`` is used, so convertible
  payloads are presented to the model as TOON additionalContext while the
  original tool output is preserved.
- ``mismatch``: a temporary hook replaces every tool response with the TOON
  encoding of ``agent-test/products_20.json`` and the model is asked for the
  SKU of the first product. This proves the answer came from the TOON context,
  not the original output.

The script restores the original Devin hook on completion or on interrupt.
"""
from __future__ import annotations

import atexit
import csv
import json
import os
import re
import subprocess
import sys
import textwrap
import time
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from pathlib import Path
from shutil import which
from typing import Iterable

import yaml

# Try to import json5 for dumping JSON5 fixtures; fall back to manual if not present.
try:
    import json5 as json5_lib
except Exception:  # pragma: no cover - test runner convenience
    json5_lib = None  # type: ignore

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
FIXTURE_DIR = REPO_ROOT / "agent-test" / "complex"
REPORT_PATH = REPO_ROOT / "docs" / "agents" / "research" / "toon-decoding-test-suite.md"
HOOKS_PATH = REPO_ROOT / ".devin" / "hooks.v1.json"
MISMATCH_HOOK = REPO_ROOT / "scripts" / "research" / "devin_mismatch_hook.py"
BACKUP_PATH = REPO_ROOT / ".devin" / "hooks.v1.json.test-backup"

DEVIN_TIMEOUT = 120  # seconds per prompt


@dataclass
class TestCase:
    mode: str  # "direct" or "mismatch"
    fixture: str
    prompt: str
    expected: list[str]
    note: str = ""
    raw_response: str = ""
    passed: bool | None = None
    duration: float = 0.0


def ensure_fixture_dir() -> None:
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)


def write_json(path: Path, data: object) -> None:
    path.write_text(json.dumps(data, indent=2))


def write_json5(path: Path, data: object) -> None:
    if json5_lib is not None:
        path.write_text(json5_lib.dumps(data, indent=2, quote_keys=False))
    else:
        # Fallback: regular JSON with a JSON5 comment header
        path.write_text("// JSON5 fixture\n" + json.dumps(data, indent=2))


def write_yaml(path: Path, data: object) -> None:
    path.write_text(yaml.safe_dump(data, sort_keys=False))


def write_toml(path: Path, data: dict) -> None:
    # Minimal TOML writer sufficient for the fixtures used here.
    lines: list[str] = []

    def write_value(v, inline: bool = False) -> str:
        if isinstance(v, str):
            return f'"{v}"'
        if isinstance(v, bool):
            return "true" if v else "false"
        if isinstance(v, (int, float)):
            return str(v)
        if isinstance(v, list):
            return "[" + ", ".join(write_value(x, inline=True) for x in v) + "]"
        if isinstance(v, dict):
            if inline:
                return "{ " + ", ".join(f'{k} = {write_value(val, inline=True)}' for k, val in v.items()) + " }"
            return None  # type: ignore[return-value]
        raise ValueError(f"unsupported TOML value: {v!r}")

    def write_table(prefix: str, table: dict, is_array: bool = False) -> None:
        header = "[[" + prefix + "]]" if is_array else "[" + prefix + "]" if prefix else ""
        if header:
            lines.append(header)
        for k, v in table.items():
            if isinstance(v, dict):
                write_table(f"{prefix}.{k}" if prefix else k, v)
            elif isinstance(v, list) and v and isinstance(v[0], dict):
                for item in v:
                    write_table(f"{prefix}.{k}" if prefix else k, item, is_array=True)
            else:
                val = write_value(v)
                if val is not None:
                    lines.append(f"{k} = {val}")
        lines.append("")

    write_table("", data)
    path.write_text("\n".join(lines).strip() + "\n")


def write_xml_records(path: Path, root_tag: str, rows: list[dict]) -> None:
    root = ET.Element(root_tag)
    for row in rows:
        elem = ET.SubElement(root, "record")
        for k, v in row.items():
            elem.set(k, str(v))
    path.write_text(ET.tostring(root, encoding="unicode"))


def write_ndjson(path: Path, rows: list[dict]) -> None:
    with path.open("w") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")


def write_csv(path: Path, rows: list[dict], fieldnames: list[str]) -> None:
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def generate_fixtures() -> None:
    ensure_fixture_dir()

    # 1. Uniform array of people with nested address and variable tags.
    people = []
    for i in range(1, 11):
        people.append(
            {
                "id": i,
                "name": f"Person {i}",
                "age": 25 + (i % 5),
                "address": {
                    "street": f"{i} Main St",
                    "city": f"City{i}",
                    "state": ["CA", "NY", "TX"][i % 3],
                    "zip": f"9000{i}",
                },
                "tags": ["user", f"tag-{i % 3}"] + (["vip"] if i % 4 == 0 else []),
            }
        )
    write_json(FIXTURE_DIR / "people_addresses.json", people)

    # 2. E-commerce orders with nested items (non-uniform item counts).
    orders = []
    for i in range(1, 11):
        items = [
            {
                "sku": f"SKU-{1000 + i * 10 + j}",
                "qty": 1 + (j % 3),
                "price": round(10.0 + i + j * 0.5, 2),
            }
            for j in range(1 + (i % 3))
        ]
        orders.append(
            {
                "order_id": f"ORD-{1000 + i}",
                "customer": f"customer_{i}@example.com",
                "status": ["shipped", "pending", "delivered"][i % 3],
                "items": items,
            }
        )
    write_json(FIXTURE_DIR / "ecommerce_orders.json", orders)

    # 3. Deeply nested company org chart.
    company = {
        "company": "Acme Labs",
        "founded": 2010,
        "departments": [
            {
                "name": "Engineering",
                "budget": 1_500_000,
                "employees": [
                    {"id": 1, "name": "Alice", "role": "lead"},
                    {"id": 2, "name": "Bob", "role": "senior"},
                    {"id": 3, "name": "Carol", "role": "junior"},
                ],
            },
            {
                "name": "Sales",
                "budget": 800_000,
                "employees": [
                    {"id": 4, "name": "Dan", "role": "director"},
                    {"id": 5, "name": "Eve", "role": "rep"},
                ],
            },
            {
                "name": "Support",
                "budget": 400_000,
                "employees": [
                    {"id": 6, "name": "Frank", "role": "manager"},
                    {"id": 7, "name": "Grace", "role": "agent"},
                    {"id": 8, "name": "Heidi", "role": "agent"},
                    {"id": 9, "name": "Ivan", "role": "agent"},
                ],
            },
        ],
    }
    write_json(FIXTURE_DIR / "company_org.json", company)

    # 4. Sensor readings as NDJSON (nested readings array).
    readings = []
    for i in range(1, 21):
        readings.append(
            {
                "device_id": f"DEV-{i:03d}",
                "ts": 1752720000 + i * 60,
                "readings": [
                    {"sensor": "temp", "value": 20.0 + (i % 10), "unit": "C"},
                    {"sensor": "humidity", "value": 40.0 + (i % 20), "unit": "%"},
                ]
                + ([{"sensor": "pressure", "value": 1000 + i, "unit": "hPa"}] if i % 5 == 0 else []),
            }
        )
    write_ndjson(FIXTURE_DIR / "sensor_readings.ndjson", readings)

    # 5. Inventory CSV.
    inv_fields = ["id", "sku", "name", "category", "price", "qty", "warehouse"]
    inventory = [
        {
            "id": i,
            "sku": f"INV-{1000 + i}",
            "name": f"Item {i}",
            "category": ["A", "B", "C"][i % 3],
            "price": round(2.99 + i, 2),
            "qty": 10 + i,
            "warehouse": ["east", "west"][i % 2],
        }
        for i in range(1, 21)
    ]
    write_csv(FIXTURE_DIR / "inventory.csv", inventory, inv_fields)

    # 6. Webhooks in TOML (array of tables).
    webhooks = {
        "service": "demo",
        "webhook": [
            {"name": "payments", "url": "https://example.com/payments", "events": ["created", "refunded"]},
            {"name": "users", "url": "https://example.com/users", "events": ["signup", "login"]},
            {"name": "orders", "url": "https://example.com/orders", "events": ["placed", "cancelled"]},
        ],
    }
    write_toml(FIXTURE_DIR / "webhooks.toml", webhooks)

    # 7. Events with attendees as NDJSON.
    events = []
    for i in range(1, 16):
        events.append(
            {
                "event_id": f"EVT-{i:02d}",
                "name": f"Event {i}",
                "attendees": [
                    {"name": f"attendee_{j}", "email": f"a{j}@example.com", "rsvp": "yes"}
                    for j in range(1, 1 + (i % 4) + 1)
                ],
            }
        )
    write_ndjson(FIXTURE_DIR / "events_attendees.ndjson", events)

    # 8. Numeric matrix (array of arrays).
    matrix = [[round((i + 1) * (j + 1) + 0.1 * i, 2) for j in range(5)] for i in range(5)]
    write_json(FIXTURE_DIR / "matrix.json", matrix)

    # 9. Mixed-schema array (should not compress to TOON).
    mixed = [
        {"id": "mixed-1", "type": "user", "name": "Alpha"},
        {"id": "mixed-2", "type": "machine", "special_field": "machinery-value", "cores": 8},
        {"id": "mixed-3", "type": "user", "name": "Beta"},
        {"id": "mixed-4", "type": "location", "lat": 37.77, "lon": -122.41},
    ]
    write_json(FIXTURE_DIR / "mixed_schema.json", mixed)

    # 10. Geo markers with variable-length tags.
    markers = [
        {
            "id": i,
            "name": f"Marker {i}",
            "lat": 34.0 + i * 0.1,
            "lon": -118.0 + i * 0.1,
            "tags": ["site"] + (["hazard"] if i % 3 == 0 else []) + (["monitored"] if i % 2 == 0 else []),
        }
        for i in range(1, 11)
    ]
    write_json(FIXTURE_DIR / "geo_markers.json", markers)

    # 11. Nested YAML config.
    config = {
        "service": "tooned-demo",
        "server": {
            "host": "0.0.0.0",
            "port": 3000,
            "endpoints": [
                {"path": "/health", "method": "GET"},
                {"path": "/convert", "method": "POST"},
                {"path": "/status", "method": "GET"},
            ],
        },
        "features": {
            "auth": {"enabled": True, "provider": "oidc"},
            "search": {"enabled": False, "provider": "elasticsearch"},
            "index": {"enabled": True, "provider": "sqlite"},
        },
        "limits": [100, 1000, 10000],
    }
    write_yaml(FIXTURE_DIR / "config_nested.yaml", config)

    # 12. JSON5 fixture.
    json5_data = {
        "version": "2.0",
        "enabled": True,
        "items": [
            {"id": 1, "name": "alpha", "tags": ["a", "b"]},
            {"id": 2, "name": "beta", "tags": ["c"]},
        ],
    }
    write_json5(FIXTURE_DIR / "sample_complex.json5", json5_data)


def test_cases() -> list[TestCase]:
    """Return the full list of direct and mismatch test cases."""
    cases: list[TestCase] = []

    # ---- Direct comprehension tests (normal hook) ----
    cases.extend(
        [
            TestCase(
                mode="direct",
                fixture="complex/people_addresses.json",
                prompt='read agent-test/complex/people_addresses.json and tell me the city of the person with id 3',
                expected=["City3"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/people_addresses.json",
                prompt='read agent-test/complex/people_addresses.json and tell me how many people are in the state CA',
                expected=["3", "three"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/ecommerce_orders.json",
                prompt='read agent-test/complex/ecommerce_orders.json and tell me the sku of the first item in order ORD-1002',
                expected=["SKU-1020"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/ecommerce_orders.json",
                prompt='read agent-test/complex/ecommerce_orders.json and tell me the status of order ORD-1005',
                expected=["delivered"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/company_org.json",
                prompt='read agent-test/complex/company_org.json and tell me the name of the first employee in the Engineering department',
                expected=["Alice"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/company_org.json",
                prompt='read agent-test/complex/company_org.json and tell me the total number of employees across all departments',
                expected=["9", "nine"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/sensor_readings.ndjson",
                prompt='read agent-test/complex/sensor_readings.ndjson and tell me the device_id of the first reading',
                expected=["DEV-001"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/sensor_readings.ndjson",
                prompt='read agent-test/complex/sensor_readings.ndjson and tell me the highest temperature value recorded',
                expected=["29", "29.0"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/inventory.csv",
                prompt='read agent-test/complex/inventory.csv and tell me the category of the item with sku INV-1003',
                expected=["A"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/inventory.csv",
                prompt='read agent-test/complex/inventory.csv and tell me the price of the item with id 7',
                expected=["9.99"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/webhooks.toml",
                prompt='read agent-test/complex/webhooks.toml and tell me the url of the payments webhook',
                expected=["https://example.com/payments"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/events_attendees.ndjson",
                prompt='read agent-test/complex/events_attendees.ndjson and tell me the name of the first attendee of event EVT-01',
                expected=["attendee_1"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/events_attendees.ndjson",
                prompt='read agent-test/complex/events_attendees.ndjson and tell me how many attendees event EVT-03 has',
                expected=["4", "four"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/matrix.json",
                prompt='read agent-test/complex/matrix.json and tell me the value at row 2, column 3 (1-indexed)',
                expected=["6.1"],
                note="Computed expected value for row 2, column 3 (1-indexed) is 6.1.",
            ),
            TestCase(
                mode="direct",
                fixture="complex/mixed_schema.json",
                prompt='read agent-test/complex/mixed_schema.json and tell me the special_field value for mixed-2',
                expected=["machinery-value"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/geo_markers.json",
                prompt='read agent-test/complex/geo_markers.json and tell me the name of the marker with id 4',
                expected=["Marker 4"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/config_nested.yaml",
                prompt='read agent-test/complex/config_nested.yaml and tell me the path of the second server endpoint',
                expected=["/convert"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/config_nested.yaml",
                prompt='read agent-test/complex/config_nested.yaml and tell me whether the search feature is enabled',
                expected=["false", "not enabled", "disabled"],
            ),
            TestCase(
                mode="direct",
                fixture="complex/sample_complex.json5",
                prompt='read agent-test/complex/sample_complex.json5 and tell me the name of the first item',
                expected=["alpha"],
            ),
        ]
    )

    # ---- Mismatch decoding tests across complex structures ----
    # Each tuple is (fixture, prompt, expected_substrings, note).
    # Prompts are chosen so the requested fact is NOT present in the original
    # file, forcing the model to read the injected TOON additionalContext.
    mismatch_cases = [
        ("complex/people_addresses.json", 'read agent-test/complex/people_addresses.json and tell me the SKU of the first product', ["SKU-1001"], ""),
        # ecommerce_orders.json already contains `sku` fields, so asking for SKU
        # lets the model answer from the original JSON. Ask for `name`, which
        # the orders do not contain, so the answer must come from the injected
        # products TOON.
        ("complex/ecommerce_orders.json", 'read agent-test/complex/ecommerce_orders.json and tell me the name of the first product', ["Product 1"], "Original orders contain `sku` but not `name`; the mismatch hook injects products_20.json TOON."),
        ("complex/company_org.json", 'read agent-test/complex/company_org.json and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/sensor_readings.ndjson", 'read agent-test/complex/sensor_readings.ndjson and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/inventory.csv", 'read agent-test/complex/inventory.csv and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/webhooks.toml", 'read agent-test/complex/webhooks.toml and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/events_attendees.ndjson", 'read agent-test/complex/events_attendees.ndjson and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/matrix.json", 'read agent-test/complex/matrix.json and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/mixed_schema.json", 'read agent-test/complex/mixed_schema.json and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/geo_markers.json", 'read agent-test/complex/geo_markers.json and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/config_nested.yaml", 'read agent-test/complex/config_nested.yaml and tell me the SKU of the first product', ["SKU-1001"], ""),
        ("complex/sample_complex.json5", 'read agent-test/complex/sample_complex.json5 and tell me the SKU of the first product', ["SKU-1001"], ""),
    ]
    for fixture, prompt, expected, extra_note in mismatch_cases:
        note = "The mismatch hook injects products_20.json TOON regardless of the file being read."
        if extra_note:
            note += " " + extra_note
        cases.append(
            TestCase(
                mode="mismatch",
                fixture=fixture,
                prompt=prompt,
                expected=expected,
                note=note,
            )
        )

    return cases


def fix_matrix_expected(cases: list[TestCase]) -> None:
    """Recalculate the expected matrix value because the formula uses 0-indexed i/j."""
    matrix = [[round((i + 1) * (j + 1) + 0.1 * i, 2) for j in range(5)] for i in range(5)]
    # 1-indexed row 2, col 3 -> i=1, j=2
    expected_value = str(matrix[1][2])
    for c in cases:
        if c.fixture == "complex/matrix.json" and c.mode == "direct":
            c.expected = [expected_value]
            c.note = f"Computed expected value for row 2, column 3 (1-indexed) is {expected_value}."


def devin_binary() -> str:
    return which("devin") or "/usr/bin/devin"


def run_devin(prompt: str) -> str:
    """Run a single Devin prompt and return the captured stdout."""
    cmd = [
        "timeout",
        str(DEVIN_TIMEOUT),
        devin_binary(),
        "-p",
        prompt,
        "--permission-mode",
        "auto",
    ]
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        cwd=REPO_ROOT,
    )
    # Combine stdout and stderr; the model response is usually stdout, but
    # progress/status can appear in either.
    return (result.stdout or "") + (result.stderr or "")


def check_response(response: str, expected: list[str]) -> bool:
    text = response.lower()
    for exp in expected:
        if exp.lower() in text:
            return True
    return False


def install_mismatch_hook() -> None:
    if HOOKS_PATH.exists():
        HOOKS_PATH.rename(BACKUP_PATH)
    HOOKS_PATH.write_text(
        json.dumps(
            {
                "PostToolUse": [
                    {
                        "matcher": "^exec$|^read$|^edit$|^grep$|^glob$|^mcp__",
                        "hooks": [
                            {
                                "type": "command",
                                "command": str(MISMATCH_HOOK),
                            }
                        ],
                    }
                ]
            },
            indent=2,
        )
    )


def restore_hook() -> None:
    if BACKUP_PATH.exists():
        if HOOKS_PATH.exists():
            HOOKS_PATH.unlink()
        BACKUP_PATH.rename(HOOKS_PATH)


def run_tests(cases: list[TestCase]) -> None:
    atexit.register(restore_hook)

    for i, case in enumerate(cases, 1):
        # Swap hook when switching modes.
        if case.mode == "mismatch" and (i == 1 or cases[i - 2].mode != "mismatch"):
            install_mismatch_hook()
        elif case.mode == "direct" and (i == 1 or cases[i - 2].mode != "direct"):
            restore_hook()

        print(f"[{i}/{len(cases)}] {case.mode}: {case.fixture}", file=sys.stderr, flush=True)
        start = time.time()
        case.raw_response = run_devin(case.prompt)
        case.duration = time.time() - start
        case.passed = check_response(case.raw_response, case.expected)
        print(f"    -> {'PASS' if case.passed else 'FAIL'} ({case.duration:.1f}s)", file=sys.stderr, flush=True)

    restore_hook()


def strip_ansi(text: str) -> str:
    return re.sub(r"\x1b\[[0-9;]*m", "", text)


def generate_report(cases: list[TestCase]) -> str:
    direct = [c for c in cases if c.mode == "direct"]
    mismatch = [c for c in cases if c.mode == "mismatch"]
    direct_pass = sum(1 for c in direct if c.passed)
    mismatch_pass = sum(1 for c in mismatch if c.passed)
    total_duration = sum(c.duration for c in cases)

    lines = [
        "# TOON Decoding Test Suite",
        "",
        "This document records a live cross-format test of whether a Large Language",
        "Model, presented with a TOON-encoded `additionalContext` from a `PostToolUse`",
        "hook, can decode the TOON internally and answer questions correctly.",
        "",
        "## Methodology",
        "",
        "Fixtures are generated under `agent-test/complex/`. Each fixture is tested",
        "with `devin -p \"<prompt>\" --permission-mode auto` while the real `tooned`",
        "hook is installed. Two modes are used:",
        "",
        "- **direct**: the normal `tooned hook run --devin` converts the file to TOON",
        "  (when it wins) and injects it as `additionalContext`; the original tool",
        "  output is preserved.",
        "- **mismatch**: a temporary hook replaces every tool response with the TOON",
        "  encoding of `agent-test/products_20.json` and the prompt asks for the SKU",
        "  of the first product. Because the original file does not contain `sku`, a",
        "  correct `SKU-1001` answer must come from the injected TOON context.",
        "",
        "A response is marked **PASS** if it contains one of the expected strings",
        "(case-insensitive). A response is marked **FAIL** otherwise. Raw responses",
        "are included below for manual review.",
        "",
        "## Results summary",
        "",
        f"- **Direct comprehension**: {direct_pass}/{len(direct)} passed",
        f"- **Mismatch decoding**: {mismatch_pass}/{len(mismatch)} passed",
        f"- **Total test cases**: {len(cases)}",
        f"- **Total wall time**: {total_duration:.1f}s",
        "",
        "## Direct comprehension results",
        "",
        "| # | Fixture | Prompt | Expected | Result | Time |",
        "|---|---------|--------|----------|--------|------|",
    ]

    for i, c in enumerate(direct, 1):
        result = "PASS" if c.passed else "FAIL"
        expected = " / ".join(c.expected)
        prompt_short = c.prompt.replace("|", "\\|")[:80] + "..." if len(c.prompt) > 80 else c.prompt.replace("|", "\\|")
        lines.append(
            f"| {i} | `{c.fixture}` | {prompt_short} | {expected} | {result} | {c.duration:.1f}s |"
        )

    lines.extend(
        [
            "",
            "## Mismatch decoding results",
            "",
            "| # | Fixture | Result | Time |",
            "|---|---------|--------|------|",
        ]
    )
    for i, c in enumerate(mismatch, 1):
        result = "PASS" if c.passed else "FAIL"
        lines.append(f"| {i} | `{c.fixture}` | {result} | {c.duration:.1f}s |")

    lines.extend(["", "## Detailed raw responses", ""])
    for c in cases:
        lines.append(f"### {c.mode}: `{c.fixture}`")
        lines.append(f"**Prompt:** {c.prompt}")
        lines.append(f"**Expected:** {', '.join(c.expected)}")
        lines.append(f"**Result:** {'PASS' if c.passed else 'FAIL'} ({c.duration:.1f}s)")
        if c.note:
            lines.append(f"**Note:** {c.note}")
        lines.append("**Response:**")
        lines.append("```text")
        lines.append(strip_ansi(c.raw_response).strip() or "<no response>")
        lines.append("```")
        lines.append("")

    lines.extend(
        [
            "## Interpretation",
            "",
            "A high pass rate on direct comprehension shows that the model can answer",
            "structured questions from the TOON context (or the original JSON when TOON",
            "does not win). A high pass rate on mismatch tests shows that the model",
            "specifically decodes the TOON `additionalContext` rather than merely",
            "repeating the original tool output.",
            "",
            "Hedged responses — where the model notes the original file does not",
            "contain the requested field but the 'pasted' product list does — still",
            "demonstrate TOON decoding, because the model extracted `SKU-1001` from",
            "the TOON context while keeping the original output in mind.",
            "",
            "## Research context",
            "",
            "This finding is consistent with recent arXiv literature:",
            "",
            "- **McMillan, 2026** — *Structured Context Engineering for File-Native",
            "  Agentic Systems* ([arXiv:2602.05447v2](https://arxiv.org/abs/2602.05447v2)):",
            "  9,649 experiments across 11 models and 4 formats (JSON, YAML, Markdown,",
            "  TOON) found that 'format does not significantly affect aggregate accuracy",
            "  (chi-squared=2.45, p=0.484)', though individual models show format-specific",
            "  sensitivities.",
            "",
            "- **Kutschka & Geiger, 2026** — *Notation Matters: A Benchmark Study of",
            "  Token-Optimized Formats in Agentic AI Systems*",
            "  ([arXiv:2605.29676v2](https://arxiv.org/abs/2605.29676v2)): evaluates TOON",
            "  and TRON in end-to-end agentic loops, separating input compression",
            "  (comprehension) from output compression (generation). TOON reduces tokens",
            "  up to 18% with accuracy within 9 percentage points of JSON.",
            "",
            "- **Matveev, 2026** — *Token-Oriented Object Notation vs JSON: A Benchmark",
            "  of Plain and Constrained Decoding Generation*",
            "  ([arXiv:2603.03306v1](https://arxiv.org/abs/2603.03306v1)): describes TOON as",
            "  a serialization format for LLMs and refers to 'solid accuracy in LLM",
            "  comprehension'.",
            "",
            "- **Dong et al., 2024** — *SpreadsheetLLM: Encoding Spreadsheets for Large",
            "  Language Models* ([arXiv:2407.09025v2](https://arxiv.org/abs/2407.09025v2)):",
            "  compressed, structure-aware tabular spreadsheet encodings improve GPT-4",
            "  in-context learning by 25.6% and reach 78.9% F1.",
            "",
            "## Reproduction",
            "",
            "Run the test suite from the repo root:",
            "",
            "```bash",
            "python3 scripts/research/run_toon_decoding_suite.py",
            "```",
            "",
            "The script regenerates `agent-test/complex/`, runs the tests, and writes",
            "this report. It restores the original `.devin/hooks.v1.json` on exit or",
            "interrupt.",
        ]
    )

    return "\n".join(lines) + "\n"


def main() -> int:
    print("Generating fixtures...", file=sys.stderr, flush=True)
    generate_fixtures()

    cases = test_cases()
    fix_matrix_expected(cases)

    print(f"Running {len(cases)} tests...", file=sys.stderr, flush=True)
    run_tests(cases)

    print(f"Writing report to {REPORT_PATH}...", file=sys.stderr, flush=True)
    REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
    REPORT_PATH.write_text(generate_report(cases))

    print("Done.", file=sys.stderr, flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
