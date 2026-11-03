#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""One GUI process, two document handles through the real stdio/socket bridge.

The platform UI harness owns the GUI lock and launches the application with
scenario(work), --mcp-folder work and an isolated discovery/settings directory.
Run this client concurrently: ui-mcp-documents-test.py <mitcad-cli> <work>.
Only synthetic boxes and files under work are used; no window/model test setter.
"""
import importlib.util
import json
import os
from pathlib import Path
import sys
import time


def scenario(work):
    work = Path(work).resolve()
    gate = lambda name: "expect-file '" + str(work / name) + "' 90"
    return "\n".join([
        "wait-idle", "capture-document-window first", gate("mcp-first-ready"),
        "wait-idle", "capture-document-window second",
        "expect-document-windows 2", gate("mcp-two-ready"),
        "focus-document-window first", gate("mcp-first-active"),
        "focus-document-window second", gate("mcp-second-active"),
        "command file.close", "expect-document-window-gone second",
        "expect-document-windows 1", gate("mcp-finished"), "quit", ""])


def run(cli, work):
    work = Path(work).resolve()
    spec = importlib.util.spec_from_file_location("mcp_documents_live", Path(__file__).with_name("ui-mcp-live.py"))
    live = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(live)
    discovery = Path(os.environ["MITCAD_MCP_DISCOVERY_DIR"])

    def wait(predicate, label):
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            if predicate():
                return
            time.sleep(0.05)
        raise AssertionError("Timed out: " + label)

    wait(lambda: len(list(discovery.glob("*.json"))) == 1, "one process listener")
    bridge = live.Bridge(cli, os.environ, work / "mcp-documents.stderr.log")
    try:
        bridge.initialize()
        documents = lambda: bridge.tool("document_list", {})["documents"]
        initial = documents()
        live.check(len(initial) == 1, "Initial process has one document")
        first = initial[0]["document"]

        def wait_editable(document):
            # Discovery and initialization announce the process, not the end
            # of native presentation or a document's modal transition.
            latest = []

            def ready():
                nonlocal latest
                latest = documents()
                current = next((item for item in latest if item["document"] == document), None)
                live.check(current is not None, "Document disappeared while waiting: " + repr(latest))
                return current["editable"] and not current["busy"]

            try:
                wait(ready, "document editable: " + document)
            except AssertionError as error:
                state = bridge.tool("ui_state", {"document": document})
                raise AssertionError(str(error) + "; documents=" + repr(latest) + "; ui_state=" + repr(state)) from error
            print("Ready for editing: " + document, flush=True)

        def query(document, value):
            arguments = {"query": value}
            if document is not None:
                arguments["document"] = document
            return bridge.tool("model_query", arguments)

        def budget(document):
            return query(document, {"query": "cache"})["result"]["memory"]["budget"]

        def volume(document, expected):
            answer = query(document, {"query": "bodies", "properties": True})
            bodies = answer["result"]
            live.check(len(bodies) == 1 and abs(bodies[0]["volume"] - expected) <= 1e-6,
                       "Wrong document geometry: " + repr(answer))
            return answer["document"]

        def box(document, length, width, height):
            bridge.tool("model_command", {"document": document, "command": {"cmd": "add_feature", "def": {
                "type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0],
                "length": length, "width": width, "height": height, "operation": "new_body"}}})

        def save(document, name):
            bridge.tool("document_save", {"document": document, "path": str(work / name), "overwrite": True})

        wait_editable(first)
        single_budget = budget(first)
        box(first, 10, 20, 30)
        save(first, "first.mitcad")
        opened = bridge.tool("ui_command", {"document": first, "id": "window.new"})
        live.check(opened["document"] == first, "New Window changed the request's original target")
        (work / "mcp-first-ready").touch()
        wait(lambda: len(documents()) == 2, "second document in same listener")
        second = next(item["document"] for item in documents() if item["document"] != first)
        wait_editable(second)
        box(second, 5, 5, 5)
        volume(first, 6000)
        volume(second, 125)
        live.check(0 < budget(first) < single_budget and 0 < budget(second) < single_budget
                   and budget(first) + budget(second) <= single_budget, "Cache budget multiplied by windows")
        bridge.tool("model_command", {"document": second, "command": {"cmd": "undo"}})
        live.check(query(second, {"query": "bodies"})["result"] == [], "Second undo did not remove its box")
        volume(first, 6000)
        bridge.tool("model_command", {"document": second, "command": {"cmd": "redo"}})
        volume(second, 125)
        save(second, "second.mitcad")
        # Even identical empty comments carry the selected document's identity.
        comments = bridge.request("resources/read", {"uri": "mitcad://comments"})
        live.check(json.loads(comments["result"]["contents"][0]["text"])["document"] == second,
                   "Comments resource belongs to another document")
        (work / "mcp-two-ready").touch()
        wait(lambda: any(item["document"] == first and item["active"] for item in documents()), "first active")
        live.check(volume(None, 6000) == first, "Omitted handle did not follow first activation")
        volume(second, 125)
        (work / "mcp-first-active").touch()
        wait(lambda: any(item["document"] == second and item["active"] for item in documents()), "second active")
        live.check(volume(None, 125) == second, "Omitted handle did not follow second activation")
        volume(first, 6000)
        (work / "mcp-second-active").touch()
        wait(lambda: len(documents()) == 1, "closing only second document")
        live.check(documents()[0]["document"] == first, "Closing second replaced first handle")
        stale = bridge.tool("model_query", {"document": second, "query": {"query": "bodies"}}, error=True)
        live.check("closed" in stale["error"].lower(), "Closed handle did not produce a stale-document refusal")
        wait(lambda: budget(first) == single_budget, "remaining document receives full cache budget")
        volume(first, 6000)
        live.check(len(list(discovery.glob("*.json"))) == 1, "Window close destroyed or duplicated process listener")
        bridge.finish()
        (work / "mcp-finished").touch()
        print("PASS: one listener, two handles, independent undo, active fallback, comments, close and cache budget")
    finally:
        bridge.stop()


if __name__ == "__main__":
    run(*sys.argv[1:3])
