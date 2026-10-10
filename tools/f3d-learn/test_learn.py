# SPDX-License-Identifier: MIT
"""Unit tests of learn.py on a synthetic dataset (no corpus data).

Run: python3 tools/f3d-learn/test_learn.py
"""

import contextlib
import io
import json
import os
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import learn  # noqa: E402

PROFILES = "item>0D57BD2F#0"


def header(id_):
    tag = b"300"
    return struct.pack("<I", len(tag)) + tag + struct.pack("<Q", id_) + struct.pack("<I", 0)


def obj(path, id_, body, cls="0D57BD2F-0000-0000-0000-000000000000"):
    """An object: header, empty root part (2 bytes), body."""
    h = header(id_)
    data = h + b"\0\0" + body
    tokens = [
        {"at": 0, "end": len(h), "t": "header"},
        {"at": len(h), "end": len(h) + 2, "t": "root", "refs": [], "attrs": {}},
    ]
    return {"path": path, "id": id_, "class": cls, "module": "", "version": 1, "len": len(data),
            "sub": 0, "hex": data.hex(), "tokens": tokens}


def example(n, chosen, flip):
    """Extrusion n of a sketch with three circles c0..c2 (one region
    each); region `chosen` is selected. The record's profile object holds
    u32 1 (the count), the chosen curve's primary id and filler; the item
    holds u8 flip."""
    base = 100 + 10 * n
    curves = {"c%d" % i: {"object": 50 + i, "class": "F0130424",
                           "attrs": {"crv_primary_id": base + 2 * i, "crv_secondary_id": 0}}
              for i in range(3)}
    regions = [{"key": "r{c%d}" % i, "outer": ["c%d" % i], "holes": [], "area": 1.0 + i,
                "centroid": [i, 0], "inside": [i, 0]} for i in range(3)]
    body = struct.pack("<II", 1, base + 2 * chosen) + struct.pack("<I", 7 * n + 3)
    record = {"long_refs": False, "objects": [
        obj("item", 10 + n, bytes([1 if flip else 0]) + b"\x55" * 7, "DD405BC2-0000-0000-0000-000000000000"),
        obj(PROFILES, 20 + n, body),
    ]}

    def cand(r, f):
        return {"defs": [{"type": "extrude", "profiles": [{"sketch": "T0", "region": r}], "flip": f,
                          "operation": "new_body", "extent": {"type": "distance", "distance": "d1"}}],
                "note": None, "guess": False, "predicted": None}

    cands = [cand(r, f) for r in range(3) for f in (False, True)]
    answer = cand(chosen, flip)
    return {"schema": 1, "file": "synthetic.f3d", "item": n, "name": "Extrude%d" % n,
            "type": "ExtrudeFeature", "class": "DD405BC2", "class_version": 8, "object_id": 10 + n,
            "status": "settled", "outcome": "parametric", "known": {"detail": {}},
            "record": record,
            "context": {"sketches": {"T0": {"index": 0, "regions": regions, "curves": curves}},
                        "bodies": [], "edges": {}},
            "candidates": cands, "accepted": cands.index(answer), "answer": answer,
            "cost": {"candidates": len(cands), "rank": cands.index(answer), "seconds": 0.5}}


def run(*argv):
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        code = learn.main(list(argv))
    return code, out.getvalue()


class LearnTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.data = cls.tmp.name
        os.makedirs(os.path.join(cls.data, "settled"))
        os.makedirs(os.path.join(cls.data, "unsettled"))
        with open(os.path.join(cls.data, "settled", "synthetic.jsonl"), "w") as f:
            for n, (chosen, flip) in enumerate([(0, False), (2, True), (1, True), (2, False), (0, True)]):
                f.write(json.dumps(example(n, chosen, flip)) + "\n")
        unsettled = example(9, 1, False)
        unsettled.update(status="unsettled", answer=None, accepted=None)
        with open(os.path.join(cls.data, "unsettled", "synthetic.jsonl"), "w") as f:
            f.write(json.dumps(unsettled) + "\n")

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def test_addresses_and_gaps(self):
        ex = learn.load(self.data, ["ExtrudeFeature"])[1]
        o = ex.obj(PROFILES)
        self.assertEqual(len(o.gaps), 1)
        self.assertEqual(ex.read(PROFILES, "g0+0"), 1)
        self.assertEqual(ex.read(PROFILES, "g0+4"), 100 + 10 + 4)
        self.assertEqual(ex.read(PROFILES, "g0-4"), 10)
        self.assertEqual(ex.read(PROFILES, "g1+0"), None)
        self.assertEqual(learn.answer_regions(ex), (2,))
        self.assertEqual(learn.answer_curves(ex), ("c2",))
        self.assertEqual(learn.target(ex, "flip"), True)

    def test_opaque_recipe_tail_addresses_keep_existing_gap_addresses(self):
        raw = obj("item", 7, struct.pack("<II", 9, 0x12345678))
        before = learn.Obj(raw)
        raw["recipe"] = {
            "kind": "edge", "entities": [], "secondary_entities": [],
            "tail_offset": len(bytes.fromhex(raw["hex"])) - 4,
            "tail_hex": "78563412", "tail_interpretation": "unverified",
        }
        after = learn.Obj(raw)
        self.assertEqual(after.gaps, before.gaps)
        self.assertEqual(after.read("g0+0"), 9)
        self.assertEqual(after.read("tail+0"), 0x12345678)
        self.assertIsNone(before.read("tail+0"))
        self.assertIsNone(after.read("tail+1"))  # u32 would exceed the tail
        self.assertIsNone(after.read("tail+4", "u8"))
        self.assertIsNone(after.read("tail+-1", "u8"))
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            learn.show_object(after)
        self.assertIn("opaque recipe tail", output.getvalue())
        self.assertIn("unverified", output.getvalue())

    def test_a_true_rule_explains_every_example(self):
        code, out = run("test", "--data", self.data, "--kind", "ExtrudeFeature", "--target", "regions",
                        "--expr", "regions_with_curves(curve_by('crv_primary_id', u32('%s', 'g0+4')))" % PROFILES,
                        "--unsettled")
        self.assertEqual(code, 0, out)
        self.assertRegex(out, r"explained\s+5\n")
        self.assertIn("among candidates", out)

    def test_a_wrong_rule_lists_counterexamples(self):
        code, out = run("test", "--data", self.data, "--kind", "ExtrudeFeature", "--target", "regions",
                        "--expr", "(0,)")
        self.assertEqual(code, 1)
        self.assertRegex(out, r"contradicted\s+3\n")
        self.assertIn("synthetic.f3d:1", out)

    def test_checks_and_rule_files(self):
        code, out = run("test", "--data", self.data, "--kind", "ExtrudeFeature",
                        "--check", "u8('item', 'g0+0') == int(target('flip'))")
        self.assertEqual(code, 0, out)
        with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as f:
            f.write("def predict(ex):\n    return ex.read('item', 'g0+0', 'u8') == 1\n")
        try:
            code, out = run("test", "--data", self.data, "--kind", "ExtrudeFeature", "--target", "flip",
                            "--rule", f.name)
        finally:
            os.unlink(f.name)
        self.assertEqual(code, 0, out)

    def test_search_finds_the_planted_fields(self):
        code, out = run("search", "--data", self.data, "--kind", "ExtrudeFeature", "--target", "regions")
        self.assertEqual(code, 0)
        rows = [line.split() for line in out.splitlines()[1:]]
        idin = [r for r in rows if r[0] == "idin"]
        self.assertEqual(idin[0][1:5], [PROFILES, "g0+4", "u32", "5"], out)
        eq = [r for r in rows if r[0] == "eq"]
        self.assertEqual(eq[0][1:5], [PROFILES, "g0+0", "u32", "5"], out)
        code, out = run("search", "--data", self.data, "--kind", "ExtrudeFeature", "--target", "flip",
                        "--families", "map")
        first = out.splitlines()[1].split()
        self.assertEqual(first[:4], ["map", "item", "g0+0", "u8"], out)

    def test_parallel_runs_print_the_same(self):
        for command in (["search", "--target", "regions"], ["survey", "--target", "flip"]):
            args = command + ["--data", self.data, "--kind", "ExtrudeFeature"]
            one = run(*args, "--jobs", "1")
            four = run(*args, "--jobs", "4")
            self.assertEqual(one, four)

    def test_survey_show_fields_and_stats(self):
        code, out = run("survey", "--data", self.data, "--kind", "ExtrudeFeature", "--target", "regions")
        self.assertEqual(code, 0)
        self.assertIn(PROFILES, out)
        self.assertIn("g0+4", out)
        code, out = run("show", "--data", self.data, "--kind", "ExtrudeFeature", "--n", "1")
        self.assertIn("answer regions: [0]", out)
        self.assertIn("g0 [12 bytes]", out)
        code, out = run("fields", "--data", self.data, "--kind", "ExtrudeFeature", "--path", PROFILES)
        self.assertIn("g0[1,114,10]", out)
        code, out = run("stats", "--data", self.data)
        self.assertIn("ExtrudeFeature", out)
        self.assertRegex(out, r"ExtrudeFeature\s+5\s+1\s")


if __name__ == "__main__":
    unittest.main()
