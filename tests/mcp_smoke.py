#!/usr/bin/env python3
"""
MCP smoke tests for evepi-server.

Spawns the server as a subprocess (stdin/stdout JSON-RPC), runs a fixed set
of tool calls against an empty (or temp) database, and asserts the responses
match expectations.

Usage:
    # Against a fresh temp DB (default — safe to run any time):
    RUST_LOG=info python3 tests/mcp_smoke.py

    # Optionally point at the release build for speed:
    EVEPI_BIN=./target/release/evepi-server python3 tests/mcp_smoke.py

    # Capture server stderr to a log file:
    RUST_LOG=info python3 tests/mcp_smoke.py 2>server.log
"""

import json
import os
import subprocess
import sys
import tempfile
import time

BINARY = os.environ.get("EVEPI_BIN", "cargo")
CARGO_ARGS = ["run", "--quiet", "--", "serve"]

PASS = "\033[32mPASS\033[0m"
FAIL = "\033[31mFAIL\033[0m"

_id_counter = 0


def next_id() -> int:
    global _id_counter
    _id_counter += 1
    return _id_counter


def send(proc, method: str, params=None) -> dict:
    """Send a JSON-RPC request and read one response line."""
    req = {"jsonrpc": "2.0", "id": next_id(), "method": method}
    if params is not None:
        req["params"] = params
    line = json.dumps(req) + "\n"
    proc.stdin.write(line.encode())
    proc.stdin.flush()
    raw = proc.stdout.readline()
    if not raw:
        raise RuntimeError("Server closed stdout unexpectedly")
    return json.loads(raw.decode())


def call_tool(proc, tool_name: str, arguments: dict | None = None) -> dict:
    """Call an MCP tool and return the parsed result content."""
    resp = send(
        proc,
        "tools/call",
        {"name": tool_name, "arguments": arguments or {}},
    )
    if "error" in resp:
        return {"__rpc_error__": resp["error"]}
    # MCP tools/call result: {"content": [{"type": "text", "text": "..."}]}
    content = resp.get("result", {}).get("content", [])
    if content and content[0].get("type") == "text":
        try:
            return json.loads(content[0]["text"])
        except json.JSONDecodeError:
            return {"__raw__": content[0]["text"]}
    return resp.get("result", {})


def assert_test(name: str, passed: bool, detail: str = "") -> bool:
    status = PASS if passed else FAIL
    print(f"  [{status}] {name}" + (f": {detail}" if detail else ""))
    return passed


def run_smoke_tests(proc) -> int:
    """Returns number of failures."""
    failures = 0

    print("\n--- MCP Smoke Tests ---\n")

    # 1. initialize handshake
    resp = send(
        proc,
        "initialize",
        {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "smoke-test", "version": "1.0"},
        },
    )
    ok = "result" in resp and "serverInfo" in resp["result"]
    if not assert_test("initialize handshake returns serverInfo", ok, str(resp.get("result", {}).get("serverInfo", ""))):
        failures += 1

    # Send initialized notification (no response expected)
    proc.stdin.write((json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n").encode())
    proc.stdin.flush()

    # 2. tools/list — all 7 tool names present
    tools_resp = send(proc, "tools/list")
    tool_names = {t["name"] for t in tools_resp.get("result", {}).get("tools", [])}
    expected_tools = {
        "sync_characters",
        "list_colonies",
        "get_colony_layout",
        "get_expiring_programs",
        "suggest_schedule",
        "set_poco_tax",
        "analyse_colony",
    }
    missing = expected_tools - tool_names
    if not assert_test("tools/list returns all 7 tools", not missing, f"missing: {missing}"):
        failures += 1

    # 3. list_colonies — empty DB returns empty list
    result = call_tool(proc, "list_colonies", {})
    ok = "colonies" in result and result["colonies"] == []
    if not assert_test("list_colonies returns empty list on fresh DB", ok, str(result)):
        failures += 1

    # 4. get_expiring_programs — empty DB returns empty list
    result = call_tool(proc, "get_expiring_programs", {"hours": 24})
    ok = "expiring" in result and result["expiring"] == []
    if not assert_test("get_expiring_programs returns empty list on fresh DB", ok, str(result)):
        failures += 1

    # 5. suggest_schedule — empty DB returns empty schedule
    result = call_tool(proc, "suggest_schedule", {})
    ok = "schedule" in result and result["schedule"] == []
    if not assert_test("suggest_schedule returns empty schedule on fresh DB", ok, str(result)):
        failures += 1

    # 6. sync_characters — no enrolled characters, expect a structured error or synced=0
    result = call_tool(proc, "sync_characters", {})
    ok = "error" in result or result.get("synced", 0) == 0
    if not assert_test("sync_characters with no characters returns error or synced=0", ok, str(result)):
        failures += 1

    # 7. set_poco_tax — upsert a phantom planet, verify rate is stored
    result = call_tool(proc, "set_poco_tax", {"planet_id": 99999, "tax_rate": 0.05})
    ok = "error" not in result and abs(result.get("tax_rate", -1) - 0.05) < 1e-9
    if not assert_test("set_poco_tax stores custom tax rate", ok, str(result)):
        failures += 1

    # 8. get_colony_layout — unknown IDs, expect structured error
    result = call_tool(proc, "get_colony_layout", {"character_id": 1, "planet_id": 99999})
    ok = "error" in result
    if not assert_test("get_colony_layout returns error for unknown colony", ok, str(result)):
        failures += 1

    # 9. analyse_colony — requires GitHub token; expect error (not a panic)
    result = call_tool(proc, "analyse_colony", {"character_id": 1, "planet_id": 99999})
    ok = "error" in result
    if not assert_test("analyse_colony returns error when GitHub token absent", ok, str(result)):
        failures += 1

    print()
    total = 9
    passed = total - failures
    print(f"Results: {passed}/{total} passed\n")
    return failures


def main() -> int:
    # Use a temporary DB so tests never touch the real one.
    with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as tmp:
        test_db_path = tmp.name
    os.unlink(test_db_path)  # Let server create it fresh

    env = dict(os.environ)
    env["EVEPI_DB_PATH"] = test_db_path
    env.setdefault("RUST_LOG", "warn")

    if BINARY == "cargo":
        cmd = ["cargo"] + CARGO_ARGS
    else:
        cmd = [BINARY, "serve"]

    print(f"Starting server: {' '.join(cmd)}")
    print(f"Test DB: {test_db_path}")

    proc = subprocess.Popen(
        cmd,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,  # Inherit stderr so server logs show in terminal (or redirect via shell)
        env=env,
        cwd=os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    )

    # Give the server a moment to start up.
    time.sleep(2)

    failures = 0
    try:
        failures = run_smoke_tests(proc)
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        # Clean up temp DB.
        if os.path.exists(test_db_path):
            os.unlink(test_db_path)

    return 0 if failures == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
