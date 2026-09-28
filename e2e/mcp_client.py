"""A minimal MCP client over a server's stdio, shared by the e2e harnesses.

Python harnesses import it as a sibling module; the heredoc harnesses put
this directory on sys.path first.
"""
import json
import subprocess

PROTOCOL_VERSION = "2025-06-18"


class McpClient:
    """Starts `argv` and completes the initialize handshake as `name`."""

    def __init__(self, argv, name, env=None, stderr=None):
        self.p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr,
                                  text=True, bufsize=1, env=env)
        self.n = 0
        self.server_info = self.request("initialize", {
            "protocolVersion": PROTOCOL_VERSION, "capabilities": {},
            "clientInfo": {"name": name, "version": "0"}})["result"]["serverInfo"]
        self.notify("notifications/initialized")

    def _write(self, msg):
        self.p.stdin.write(json.dumps(msg) + "\n")
        self.p.stdin.flush()

    def request(self, method, params=None):
        """The response message to one request; other messages are skipped."""
        self.n += 1
        msg = {"jsonrpc": "2.0", "id": self.n, "method": method}
        if params is not None:
            msg["params"] = params
        self._write(msg)
        while True:
            line = self.p.stdout.readline()
            if not line:
                raise RuntimeError("MCP server exited")
            reply = json.loads(line)
            if reply.get("id") == self.n:
                return reply

    def notify(self, method):
        self._write({"jsonrpc": "2.0", "method": method})

    def tool(self, name, args):
        """(True, structuredContent) on success, (False, error text) when the
        tool reports an error."""
        r = self.request("tools/call", {"name": name, "arguments": args})["result"]
        if r.get("isError"):
            return False, "".join(c.get("text", "") for c in r.get("content") or [])
        return True, r.get("structuredContent")

    def close(self, timeout=10):
        self.p.stdin.close()
        self.p.wait(timeout=timeout)
