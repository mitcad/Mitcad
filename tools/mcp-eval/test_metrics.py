# SPDX-License-Identifier: MIT
"""Tests of the evaluation's metric extractor (mitcad#1507).

Usage: python3 -I test_metrics.py --cli <mitcad-cli> --work <empty folder>

A small design with known values is made and saved through the headless
MCP server; the extractor must read those values back from the saved file.
The transcript readers are tested on short synthetic transcripts.
"""

import argparse
import json
import os
import shutil
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import metrics  # noqa: E402
from mcp_client import Server  # noqa: E402

CLI = None
WORK = None


def box(plane, length, width, height):
    return {"cmd": "add_feature", "def": {
        "type": "box", "plane": plane, "corner": [0, 0], "length": length,
        "width": width, "height": height, "operation": "new_body"}}


def make_design(workspace):
    """Saves design.mitcad in the workspace; its metrics are in DesignTests."""
    with Server(CLI, workspace) as server:
        document = server.tool("document_create", {})["document"]
        server.command(document, {"cmd": "add_parameter", "name": "r", "value": 9.5})
        server.command(document, {"cmd": "add_parameter", "name": "count", "value": 4})
        # Box1: 20 x 20 x 30 on the origin plane; Fillet1 rounds its four
        # 30 mm edges by r, which a 20 mm side takes up to r = 10.
        server.command(document, box("xy", 20, 20, 30))
        edges = server.query(document, {"query": "edges", "body": "F1.b0", "curve": "line",
                                        "min_length": 29.9})["edges"]
        server.command(document, {"cmd": "add_feature", "def": {
            "type": "fillet", "body": "F1.b0", "edges": [edge["name"] for edge in edges],
            "radius": "r"}})
        # Box2 on a fixed plane, 30 x 30 x 5.
        server.command(document, box({"origin": [0, 0, 100], "normal": [0, 0, 1]}, 30, 30, 5))
        # A sketch with a dimensioned line whose ends are free: dof 3.
        server.command(document, {"cmd": "add_feature", "name": "Outline", "def": {
            "type": "sketch", "plane": "xz",
            "entities": [{"id": "c1", "type": "line", "start": "p2", "end": "p3"},
                         {"id": "p2", "type": "point", "at": [0, 0]},
                         {"id": "p3", "type": "point", "at": [30, 0]}],
            "dimensions": [{"id": "k1", "type": "length", "line": "c1", "value": 30}]}})
        # A chamfer of an edge Box2 does not have: it fails.
        server.command(document, {"cmd": "add_feature", "name": "Broken edge", "def": {
            "type": "chamfer", "body": "F3.b0", "edges": ["E{F3:side3|F3:side9}"],
            "size": {"type": "equal_distance", "distance": 1}}})
        timeline = server.query(document, {"query": "timeline"})
        server.command(document, {"cmd": "group_features", "name": "Base", "features": [
            timeline["features"][0]["uid"], timeline["features"][1]["uid"]]})
        server.tool("document_save", {"document": document, "path": metrics.DESIGN})


CLAUDE = [
    {"type": "system", "subtype": "init", "tools": ["mcp__mitcad__model_command"]},
    {"type": "assistant", "message": {"content": [
        {"type": "text", "text": "Making it."},
        {"type": "tool_use", "id": "t1", "name": "mcp__mitcad__document_create", "input": {}}]}},
    {"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": "t1", "content": "{}"}]}},
    {"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": "t2", "name": "mcp__mitcad__model_command", "input": {}}]}},
    # The same message again, as a stream repeats it: counted once.
    {"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": "t2", "name": "mcp__mitcad__model_command", "input": {}}]}},
    {"type": "user", "message": {"content": [
        {"type": "tool_result", "tool_use_id": "t2", "is_error": True, "content": "refused"}]}},
    {"type": "result", "subtype": "success", "is_error": False, "num_turns": 3,
     "total_cost_usd": 0.12, "usage": {"input_tokens": 100, "cache_read_input_tokens": 900,
                                       "cache_creation_input_tokens": 50, "output_tokens": 70}},
]

CODEX = [
    {"type": "thread.started", "thread_id": "x"},
    {"type": "turn.started"},
    {"type": "item.completed", "item": {"id": "i1", "type": "agent_message", "text": "Making it."}},
    {"type": "item.completed", "item": {"id": "i2", "type": "mcp_tool_call", "server": "mitcad",
                                        "tool": "model_command", "status": "completed"}},
    {"type": "item.completed", "item": {"id": "i3", "type": "mcp_tool_call", "server": "mitcad",
                                        "tool": "model_command", "status": "failed",
                                        "error": {"message": "refused"}}},
    {"type": "turn.completed", "usage": {"input_tokens": 400, "cached_input_tokens": 300,
                                         "output_tokens": 20}},
]


def write_lines(path, events):
    with open(path, "w", encoding="utf-8") as out:
        for event in events:
            out.write(json.dumps(event) + "\n")
        out.write("not json\n")


class RuleTests(unittest.TestCase):
    def test_default_names(self):
        self.assertTrue(metrics.default_name("Extrude3", "extrude"))
        self.assertTrue(metrics.default_name("CircularPattern1", "circular_pattern"))
        self.assertTrue(metrics.default_name("Plane2", "construction_plane"))
        self.assertFalse(metrics.default_name("Base plate", "extrude"))
        self.assertFalse(metrics.default_name("Extrude", "extrude"))
        self.assertFalse(metrics.default_name("Holder2", "extrude"))

    def test_fixed_plane(self):
        self.assertTrue(metrics.fixed_plane({"at": {"plane": {"origin": [0, 0, 1], "normal": [0, 0, 1]}}}))
        self.assertTrue(metrics.fixed_plane({"items": [{"origin": [0, 0, 0], "normal": [1, 0, 0]}]}))
        self.assertFalse(metrics.fixed_plane({"plane": "xy", "frame": {"origin": [0, 0, 0], "x_axis": [1, 0, 0]}}))
        self.assertFalse(metrics.fixed_plane({"axis": {"origin": [0, 0, 0], "direction": [0, 0, 1]}}))

    def test_repeated_literals(self):
        def parameter(name, expression, value, kind="model", unit="mm", dependencies=()):
            return {"name": name, "expression": expression, "value": value, "kind": kind,
                    "unit": unit, "dependencies": list(dependencies)}
        parameters = [
            parameter("d1", "30 mm", 30), parameter("d2", "30", 30), parameter("d3", "30 mm", 30),
            parameter("d4", "w", 30, dependencies=["w"]),
            parameter("d5", "w * 2", 60, dependencies=["w"]),
            parameter("d6", "90 deg", 1.5708, unit="deg"), parameter("d7", "90 deg", 1.5708, unit="deg"),
            parameter("d8", "90 deg", 1.5708, unit="deg"),
            parameter("w", "30 mm", 30, kind="user"),
            parameter("d9", "5 mm", 5), parameter("d10", "5 mm", 5),
        ]
        self.assertEqual(metrics.repeated_literals(parameters), {"30 mm": 3})

    def test_perturbed(self):
        self.assertAlmostEqual(metrics.perturbed(10.0, "mm"), 11.0)
        self.assertEqual(metrics.perturbed(4, ""), 5.0)
        self.assertEqual(metrics.perturbed(20, ""), 22.0)


class TranscriptTests(unittest.TestCase):
    def test_claude(self):
        path = os.path.join(WORK, "claude.jsonl")
        write_lines(path, CLAUDE)
        totals = metrics.transcript_metrics(path)
        self.assertEqual(totals["agent"], "claude")
        self.assertEqual((totals["tool_calls"], totals["tool_errors"]), (2, 1))
        self.assertEqual(totals["tools"], {"mcp__mitcad__document_create": 1,
                                           "mcp__mitcad__model_command": 1})
        self.assertEqual((totals["input_tokens"], totals["cached_input_tokens"],
                          totals["output_tokens"]), (100, 950, 70))
        self.assertEqual(totals["turns"], 3)
        self.assertTrue(totals["finished"])

    def test_codex(self):
        path = os.path.join(WORK, "codex.jsonl")
        write_lines(path, CODEX)
        totals = metrics.transcript_metrics(path)
        self.assertEqual(totals["agent"], "codex")
        self.assertEqual((totals["tool_calls"], totals["tool_errors"]), (2, 1))
        self.assertEqual(totals["tools"], {"mitcad.model_command": 2})
        self.assertEqual((totals["input_tokens"], totals["cached_input_tokens"],
                          totals["output_tokens"]), (400, 300, 20))
        self.assertTrue(totals["finished"])


class DesignTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.run_folder = os.path.join(WORK, "run")
        cls.workspace = os.path.join(cls.run_folder, "fixture", "workspace")
        os.makedirs(cls.workspace)
        make_design(cls.workspace)
        write_lines(os.path.join(cls.run_folder, "fixture", metrics.TRANSCRIPT), CLAUDE)
        with open(os.path.join(cls.run_folder, "fixture", metrics.META), "w", encoding="utf-8") as out:
            json.dump({"platform": "test platform", "commit": "abc123", "agent": "claude"}, out)
        # A task whose agent saved nothing.
        os.makedirs(os.path.join(cls.run_folder, "unsaved", "workspace"))
        cls.results = metrics.evaluate_run(CLI, cls.run_folder)

    def test_design_metrics(self):
        design = self.results["fixture"]["design"]
        self.assertEqual(design["features"], 5)
        self.assertEqual(design["sketches"], 1)
        self.assertEqual(design["underdefined_sketches"], ["Outline"])
        self.assertEqual(design["fixed_plane_features"], ["Box2"])
        # 30 mm: Box1's height, Box2's length and width, the sketch's line.
        self.assertEqual(design["repeated_literals"], {"30 mm": 4})
        self.assertEqual(design["user_parameters"], 2)
        self.assertEqual(design["default_names"], ["Box1", "Fillet1", "Box2"])
        self.assertEqual(design["groups"], 1)
        self.assertEqual(design["ungrouped_features"], 3)
        self.assertEqual(design["failed_features"], ["Broken edge"])
        changes = {change["name"]: change for change in design["parameter_changes"]}
        # r = 10.45 rounds Box1's 20 mm sides away; count is used by nothing.
        self.assertEqual(changes["r"]["outcome"], "failed")
        self.assertEqual(changes["r"]["failed"], ["Fillet1"])
        self.assertEqual(changes["count"]["outcome"], "ok")

    def test_design_unchanged(self):
        # The extractor changes parameters and undoes them; the file stays.
        with Server(CLI, self.workspace) as server:
            again = metrics.design_metrics(server, metrics.DESIGN)
        self.assertEqual(again, self.results["fixture"]["design"])

    def test_report(self):
        self.assertIsNone(self.results["unsaved"]["design"])
        with open(os.path.join(self.run_folder, "report.md"), encoding="utf-8") as source:
            report = source.read()
        self.assertIn("- Platform: test platform", report)
        self.assertIn("- MitCad commit: abc123", report)
        self.assertIn("| fixture | yes | 5 | 1/1 | 1 | 4 | 2 | 3/5 | 3/5 | 1 | 1/2 | 2 | 1 | 100 / 950 / 70 |",
                      report)
        self.assertIn("| unsaved | no |", report)
        with open(os.path.join(self.run_folder, "report.json"), encoding="utf-8") as source:
            self.assertEqual(json.load(source)["fixture"]["transcript"]["tool_calls"], 2)


def main():
    global CLI, WORK
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args, rest = parser.parse_known_args()
    CLI = os.path.abspath(args.cli)
    WORK = os.path.abspath(args.work)
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    unittest.main(argv=[sys.argv[0]] + rest)


if __name__ == "__main__":
    main()
