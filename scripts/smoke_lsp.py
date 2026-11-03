#!/usr/bin/env python3
"""Smoke-test the bundled skript-lsp binary over stdio with a real LSP
initialize handshake. Usage: smoke_lsp.py <path-to-binary>"""
import json
import subprocess
import sys
import threading

BIN = sys.argv[1]
proc = subprocess.Popen(
    [BIN, "--stdio", "--log-level", "error"],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
)


def send(msg: dict) -> None:
    body = json.dumps(msg)
    proc.stdin.write(
        f"Content-Length: {len(body)}\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{body}".encode()
    )
    proc.stdin.flush()


def read_msg() -> dict:
    headers = {}
    while True:
        line = proc.stdout.readline().decode().strip()
        if not line:
            break
        k, _, v = line.partition(":")
        headers[k.strip().lower()] = v.strip()
    return json.loads(proc.stdout.read(int(headers["content-length"])))


send(
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": None,
            "rootUri": None,
            "capabilities": {},
        },
    }
)
reply = read_msg()
caps = reply.get("result", {}).get("capabilities", {})
print("serverInfo:", reply.get("result", {}).get("serverInfo"))
for key in ("completionProvider", "hoverProvider", "definitionProvider"):
    print(f"  {key}: {'yes' if key in caps else 'no'}")

send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
send({"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": None})
read_msg()
send({"jsonrpc": "2.0", "method": "exit", "params": None})
# tower-lsp 0.20's serve loop ends on stdin EOF (the `exit` notification only
# marks the service Exited); VSCode closes stdin when stopping the client.
proc.stdin.close()
proc.wait(timeout=10)
print("exit code:", proc.returncode)
assert proc.returncode == 0, "server did not exit cleanly"
print("SMOKE OK")
