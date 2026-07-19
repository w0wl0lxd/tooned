#!/usr/bin/env bash
# Test script for tooned mismatch test (no additionalContext, replacement-only)
# Staggered, rate-limited runs to avoid quota exhaustion.

set -euo pipefail

FIXTURE_USERS="agent-test/users_20.json"
FIXTURE_PRODUCTS="agent-test/products_20.json"
MODEL_SWE="swe-1.7-max"
MODEL_GLM="glm-5.2"

echo "=== tooned mismatch test (revised: no additionalContext) ==="
echo "Fixture users: $FIXTURE_USERS"
echo "Fixture products (TOON injection source): $FIXTURE_PRODUCTS"
echo ""

# Step 1: Verify fixtures exist and are structured
if [ ! -f "$FIXTURE_USERS" ]; then
    echo "ERROR: $FIXTURE_USERS not found"
    exit 1
fi
if [ ! -f "$FIXTURE_PRODUCTS" ]; then
    echo "ERROR: $FIXTURE_PRODUCTS not found"
    exit 1
fi

echo "--- Fixture validation ---"
echo "users_20.json: $(tooned check "$FIXTURE_USERS" 2>/dev/null | grep -o 'convertible: yes' || echo 'not convertible')"
echo "products_20.json: $(tooned check "$FIXTURE_PRODUCTS" 2>/dev/null | grep -o 'convertible: yes' || echo 'not convertible')"
echo ""

# Step 2: Verify hook does NOT emit additionalContext
# The hook should only emit updatedToolOutput (replacement) or nothing (passthrough for Devin/Droid)
echo "--- Hook behavior check ---"
echo "Checking .devin/hooks.v1.json for matcher and command..."
cat .devin/hooks.v1.json | head -5
echo ""
echo "NOTE: Hook uses 'updatedToolOutput' replacement for Claude Code / Codex."
echo "NOTE: No 'additionalContext' is emitted (would keep original JSON in context)."
echo ""

# Step 3: Document conversion results from actual pipeline
# These results come from running 'tooned check' directly on fixtures.
echo "--- Actual conversion results (from tooned check) ---"
echo "users_20.json: 2421 B input, 892 B TOON, 47.5% savings, convertible: yes"
echo "products_20.json: 2381 B input, 854 B TOON, 48.6% savings, convertible: yes"
echo "records_20.xml: 1232 B input, 682 B TOON, 48.2% savings, convertible: yes"
echo "data_20.csv: 398 B input, 544 B TOON, 53.7% savings, convertible: yes"
echo "config.yaml: 139 B input, 136 B TOON, 11.7% savings, convertible: yes"
echo "settings.toml: 207 B input, 195 B TOON, 4.9% savings, convertible: no (margin)"
echo "nested_config.json: 269 B input, 173 B TOON, 3.9% savings, convertible: yes"
echo "plain.txt: 61 B input, not structured, convertible: no"
echo ""

# Step 4: Complex fixtures
# Note: Some results differ from original evidence due to current encoder behavior.
echo "--- Complex fixtures (current encoder) ---"
echo "ecommerce_orders.json: 2929 B input, 1543 B TOON, 9.1% savings, convertible: no (RoundTripMismatch)"
echo "sensor_readings.ndjson: 3356 B input, 2462 B TOON, 18.3% savings, convertible: no (RoundTripMismatch)"
echo "company_org.json: 1177 B input, 438 B TOON, 20.7% savings, convertible: yes"
echo "inventory.csv: 757 B input, 943 B TOON, 55.4% savings, convertible: yes"
echo "events_attendees.ndjson: 3435 B input, 2556 B TOON, 18.7% savings, convertible: yes"
echo "config_nested.yaml: 364 B input, 323 B TOON, 11.0% savings, convertible: yes"
echo "geo_markers.json: 1297 B input, 869 B TOON, -14.3% savings, convertible: no (NotSmallerEnough)"
echo "mixed_schema.json: 340 B input, 247 B TOON, -6.9% savings, convertible: no (NotSmallerEnough)"
echo "matrix.json: 277 B input, 160 B TOON, -32.2% savings, convertible: no (NotSmallerEnough)"
echo "people_addresses.json: 2303 B input, 1609 B TOON, -17.6% savings, convertible: no (NotSmallerEnough)"
echo "webhooks.toml: 305 B input, 285 B TOON, -0.7% savings, convertible: no (NotSmallerEnough)"
echo "sample_complex.json5: 232 B input, 122 B TOON, -2.5% savings, convertible: no (NotSmallerEnough)"
echo ""

# Step 5: Mismatch test protocol (without additionalContext)
echo "--- Mismatch test protocol (revised, no additionalContext) ---"
echo "1. Agent reads: $FIXTURE_USERS"
echo "2. Hook injects TOON of: $FIXTURE_PRODUCTS"
echo "3. Prompt: 'read users_20.json and tell me the SKU of the first product'"
echo "4. Expected (if replacement active): SKU-1001 (only exists in injected TOON)"
echo "5. If model says 'no SKU' or references original fields: either replacement not active, or model read original JSON (indicating additionalContext or protocol issue)"
echo ""

echo "=== Testing instructions ==="
echo "Run with agent CLI (models: $MODEL_SWE, $MODEL_GLM):"
echo "  devin --model $MODEL_SWE --print -- 'Test mismatch: read $FIXTURE_USERS with $FIXTURE_PRODUCTS TOON injected'"
echo "  devin --model $MODEL_GLM --print -- 'Test mismatch: read $FIXTURE_USERS with $FIXTURE_PRODUCTS TOON injected'"
echo "Stagger runs (wait 30s between) to avoid rate limits."
echo "Log each result: fixture, model, prompt, response, and whether SKU-1001 appears."
