#!/usr/bin/env python3
"""End-to-end hover verification against a real skript-lsp binary over stdio.

Reproduces the 2026-11 hover regression exactly as users saw it:
  1. initialize + didOpen            (engine compiles in background)
  2. textDocument/hover mid-word     (used to hang forever -> "Loading..")
  3. assert the reply arrives fast, highlights the WHOLE word (not one
     character), and carries real SkriptHub docs
  4. didChange + hover again         (check & hover interleave on the
     shared engine channel)

Usage:
    python3 scripts/e2e_hover_test.py [path-to-binary]

Exit code 0 = pass. Any hang is converted into a failure by the socket
timeouts, so this script is CI-safe.
"""
import json
import os
import queue
import subprocess
import sys
import threading
import time

_REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(
    _REPO, "extensions", "vscode-skript", "bin", "skript-lsp-linux-x64")

proc = subprocess.Popen(
    [BIN, "--stdio", "--log-level", "warn"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
)

# A dedicated reader thread keeps the pipe drained; select() on a
# buffered file object is unreliable (bytes hide in Python's buffer).
_messages = queue.Queue()


def _reader():
    try:
        while True:
            headers = {}
            while True:
                line = proc.stdout.readline()
                if not line or line in (b"\r\n", b"\n"):
                    break
                k, _, v = line.decode().partition(":")
                headers[k.strip().lower()] = v.strip()
            if not headers and not line:
                break  # EOF
            length = int(headers.get("content-length", 0))
            body = proc.stdout.read(length)
            _messages.put(json.loads(body))
    except Exception:
        pass
    finally:
        _messages.put(None)  # sentinel: stream is over


threading.Thread(target=_reader, daemon=True).start()

_next_id = [0]


def send(method, params, request=False):
    msg = {"jsonrpc": "2.0", "method": method, "params": params}
    if request:
        _next_id[0] += 1
        msg["id"] = _next_id[0]
    data = json.dumps(msg).encode()
    proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(data) + data)
    proc.stdin.flush()
    return msg.get("id")


def read_msg(timeout):
    """Read one LSP message, or None on timeout/EOF."""
    try:
        return _messages.get(timeout=timeout)
    except queue.Empty:
        return None


def wait_for_response(req_id, timeout):
    deadline = time.time() + timeout
    while time.time() < deadline:
        msg = read_msg(deadline - time.time())
        if msg is None:
            return None
        if msg.get("id") == req_id and ("result" in msg or "error" in msg):
            return msg
    return None


def fail(msg):
    print(f"FAIL: {msg}")
    proc.terminate()
    sys.exit(1)


# --- 1. initialize ----------------------------------------------------------
rid = send("initialize", {
    "processId": None, "rootUri": None, "capabilities": {},
}, request=True)
init = wait_for_response(rid, 15)
if init is None or "error" in init:
    fail(f"initialize failed: {init}")
print("initialized:", init["result"]["serverInfo"])
send("initialized", {})

# --- 2. didOpen -------------------------------------------------------------
script = 'on join:\n\tbroadcast "hello world"\n\tfrobnicate the widgets\n'
send("textDocument/didOpen", {
    "textDocument": {"uri": "file:///hover.sk", "languageId": "skript",
                     "version": 1, "text": script},
})

# Give the engine a moment to compile (it logs ~200ms but be generous),
# meanwhile drain diagnostics.
saw_diagnostics = False
for _ in range(4):
    msg = read_msg(5)
    if msg and msg.get("method") == "textDocument/publishDiagnostics":
        saw_diagnostics = True

# --- 3. hover mid-word: 'broadcast' at column 4 -----------------------------
t0 = time.time()
rid = send("textDocument/hover", {
    "textDocument": {"uri": "file:///hover.sk"},
    "position": {"line": 1, "character": 4},
}, request=True)
reply = wait_for_response(rid, 10)
elapsed = time.time() - t0
if reply is None:
    fail("hover never answered within 10s — the engine thread is starving "
         "hover requests (the 'Loading..' regression)")
if "error" in reply:
    fail(f"hover returned an error: {reply['error']}")
hover = reply["result"]
if hover is None:
    fail("hover returned null for 'broadcast' — expected SkriptHub docs")
rng = hover["range"]
start, end = rng["start"], rng["end"]
if start["line"] != 1 or end["line"] != 1:
    fail(f"hover range must be on line 1, got {rng}")
if (start["character"], end["character"]) != (1, 10):
    # line is '\tbroadcast "hello world"' — the word spans cols 1..10
    fail(f"hover must highlight the whole word 'broadcast' (cols 1..10), "
         f"got ({start['character']}..{end['character']}) — single-character "
         f"or full-line highlight is a regression")
contents = hover["contents"]["value"]
if "roadcast" not in contents or "effect" not in contents.lower():
    fail(f"hover contents do not look like SkriptHub docs: {contents[:120]!r}")
print(f"PASS: hover answered in {elapsed:.2f}s, word range 1..10, docs ok")

# --- 4. hover inside a string literal ----------------------------------------
# The AST path claims literals: hovering 'hello' inside "hello world"
# highlights the WHOLE string literal (cols 12..23) and shows its type —
# a word-size highlight is NOT expected here.
rid = send("textDocument/hover", {
    "textDocument": {"uri": "file:///hover.sk"},
    "position": {"line": 1, "character": 14},  # inside 'hello'
}, request=True)
reply = wait_for_response(rid, 10)
if reply is None:
    fail("second hover never answered")
rng = (reply["result"] or {}).get("range", {})
s, e = rng.get("start", {}).get("character"), rng.get("end", {}).get("character")
if (s, e) != (12, 23):
    fail(f"hover inside the string must highlight the literal (cols 12..23), "
         f"got {s}..{e}")
print("PASS: hover inside string highlights the literal (12..23)")

# --- 5. didChange then hover (interleaving check + hover) --------------------
script2 = 'on join:\n\tbroadcast "hello world"\n\tbroadcast "again"\n'
send("textDocument/didChange", {
    "textDocument": {"uri": "file:///hover.sk", "version": 2},
    "contentChanges": [{"text": script2}],
})
rid = send("textDocument/hover", {
    "textDocument": {"uri": "file:///hover.sk"},
    "position": {"line": 2, "character": 2},
}, request=True)
reply = wait_for_response(rid, 10)
if reply is None:
    fail("hover after didChange never answered — check/hover interleaving "
         "is broken on the shared engine channel")
if reply["result"] is None:
    fail("hover on the new line returned null")
rng = reply["result"]["range"]
if (rng["start"]["character"], rng["end"]["character"]) != (1, 10):
    fail(f"hover after didChange must highlight 'broadcast', got {rng}")
print("PASS: hover after didChange answered and highlighted correctly")

print(f"\nALL PASS (diagnostics seen: {saw_diagnostics})")
proc.terminate()
