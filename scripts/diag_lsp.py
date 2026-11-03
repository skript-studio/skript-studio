#!/usr/bin/env python3
"""Diagnose a .sk file through the real skript-lsp stdio pipeline.

Usage: diag_lsp.py <binary> <file.sk>

Sends initialize + didOpen, collects publishDiagnostics, prints them grouped
and deduplicated by message, then exits."""
import json
import subprocess
import sys
from collections import defaultdict

BIN, PATH = sys.argv[1], sys.argv[2]
TEXT = open(PATH, encoding="utf-8").read()
URI = "file://" + PATH

proc = subprocess.Popen(
    [BIN, "--stdio", "--log-level", "error"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
)


def send(msg):
    body = json.dumps(msg)
    proc.stdin.write(
        f"Content-Length: {len(body)}\r\n\r\n{body}".encode()
    )
    proc.stdin.flush()


notifications = []


def recv():
    headers = {}
    while True:
        line = proc.stdout.readline().decode().strip()
        if not line:
            break
        k, _, v = line.partition(":")
        headers[k.strip().lower()] = v.strip()
    return json.loads(proc.stdout.read(int(headers["content-length"])))


send({"jsonrpc": "2.0", "id": 1, "method": "initialize",
      "params": {"processId": None, "rootUri": None, "capabilities": {}}})
recv()
send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
send({"jsonrpc": "2.0", "method": "textDocument/didOpen",
      "params": {"textDocument": {"uri": URI, "languageId": "skript",
                                   "version": 1, "text": TEXT}}})

# Collect diagnostics until we see a publishDiagnostics for our file, then
# keep draining briefly in case more arrive (debounce).
import time

deadline = time.time() + 25
diags = None
while time.time() < deadline:
    msg = recv()
    if msg.get("method") == "textDocument/publishDiagnostics":
        if msg["params"]["uri"] == URI:
            diags = msg["params"]["diagnostics"]
            break

if diags is None:
    print("NO DIAGNOSTICS RECEIVED")
    sys.exit(1)

groups = defaultdict(list)
for d in diags:
    groups[d["message"]].append(d["range"]["start"]["line"] + 1)

print(f"TOTAL: {len(diags)} diagnostics")
for msg, lines in sorted(groups.items(), key=lambda kv: -len(kv[1])):
    ls = sorted(set(lines))
    show = ls if len(ls) <= 12 else ls[:12] + [f"...+{len(ls)-12} more"]
    print(f"[{len(lines):3d}x] {msg[:110]}")
    print(f"       lines: {', '.join(map(str, show))}")

proc.stdin.close()
proc.kill()
