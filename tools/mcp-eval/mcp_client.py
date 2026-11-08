# SPDX-License-Identifier: MIT
"""A small client of MitCad's headless MCP server (`mitcad-cli mcp --workspace`).

The metric extractor and its test read saved designs through it: one server
process over stdio, JSON-RPC requests one at a time, a reader thread so that
a read has a timeout on Windows as well as on Unix.
"""

import json
import queue
import subprocess
import tempfile
import threading

PROTOCOL = "2025-11-25"


class McpError(Exception):
    """A tool call the server refused, or a broken connection."""


class Server:
    """`mitcad-cli mcp --workspace <workspace>` as a context manager."""

    def __init__(self, cli, workspace, timeout=120):
        self.timeout = timeout
        self.stderr = tempfile.TemporaryFile()
        self.process = subprocess.Popen(
            [str(cli), "mcp", "--workspace", str(workspace)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.stderr)
        self.lines = queue.Queue()
        self.next_id = 0
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()
        self._initialize()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        if self.process.poll() is None:
            try:
                self.process.stdin.close()
                self.process.wait(timeout=self.timeout)
            except (OSError, subprocess.TimeoutExpired):
                self.process.kill()
                self.process.wait()
        self.reader.join(timeout=self.timeout)
        self.process.stdout.close()
        self.stderr.close()

    def _read(self):
        while True:
            line = self.process.stdout.readline()
            self.lines.put(line)
            if not line:
                return

    def _diagnostics(self):
        self.stderr.seek(0)
        return self.stderr.read().decode("utf-8", "replace")[-4000:]

    def _send(self, message):
        self.process.stdin.write(json.dumps(message, separators=(",", ":")).encode("utf-8") + b"\n")
        self.process.stdin.flush()

    def request(self, method, params=None):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            message["params"] = params
        self._send(message)
        while True:
            try:
                line = self.lines.get(timeout=self.timeout)
            except queue.Empty as error:
                raise McpError(f"{method}: no answer in {self.timeout} s") from error
            if not line:
                raise McpError(f"{method}: the server ended: {self._diagnostics()}")
            answer = json.loads(line)
            if answer.get("id") == self.next_id:
                break
            # A notification (a log line, a list change) is not the answer.
        if "error" in answer:
            raise McpError(f"{method}: {answer['error']}")
        return answer["result"]

    def _initialize(self):
        self.request("initialize", {
            "protocolVersion": PROTOCOL, "capabilities": {},
            "clientInfo": {"name": "mitcad-mcp-eval", "version": "1"}})
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def tool(self, name, arguments):
        """The structured result of a tool call; a refused call raises McpError."""
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        content = result.get("structuredContent")
        if content is None:
            text = "".join(block.get("text", "") for block in result.get("content", []))
            content = json.loads(text) if text.startswith(("{", "[")) else {"text": text}
        if result.get("isError"):
            raise McpError(f"{name}: {content.get('error', content)}")
        return content

    def query(self, document, query):
        return self.tool("model_query", {"document": document, "query": query})["result"]

    def command(self, document, command):
        """Runs a command; a refusal raises McpError, a failed feature does not."""
        result = self.request("tools/call", {"name": "model_command", "arguments": {
            "document": document, "command": command}})
        content = result.get("structuredContent") or {}
        if result.get("isError") and content.get("applied") is not True:
            raise McpError(f"model_command: {content.get('error', content)}")
        return content
