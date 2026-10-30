# SPDX-License-Identifier: MIT
"""Builds the BUILD parts of Mitcad's parts library (mitcad-parts;
mitcad#375): the BUILD tab's slot profiles, connectors, nuts, caps, panels
and linear parts as parametric Mitcad designs, with the library's
manifest, previews, licence, sources and README. With --merge the
library's other parts already in the manifest (the fasteners;
tools/libraries/make-parts-library.py runs both generators) stay. With
--commit the folder becomes a git repository with the version recorded and
tagged.

    python3 make-profile-library.py --cli build/dev/tools/cli/mitcad-cli \\
        --source <folder of the supplier's files> --out ../libraries/mitcad-parts \\
        [--groups tools/libraries/profiles] [--only id,id] [--version 1.0.0] [--merge] [--commit] \\
        [--render-worker build/dev/app/mitcad-render] [--no-render] [--rerender]

The fasteners (nuts, screws, wood fasteners) get a finish as their
appearance and a preview rendered in the studio of library_previews.py
(mitcad#526), kept when their design did not change.

The parts are listed in group files (tools/libraries/profiles/*.json; the
format is in docs/libraries.md, "Mitcad's parts library"). Each part is
made one of three ways:

- section: the cross-section at the source's end face, placed on XY at its
  centre, as a sketch extruded by the length parameter along +Z (end A at
  z = 0, end B at z = L);
- solid: the source's solids as a base feature, moved to Mitcad's
  coordinates with `place`;
- model: no source; the design is built from `dimensions` (a plate, an
  angle, a rod with a cosmetic thread, a timber board (plain, or profiled: tongue and
  groove, shiplap, feather-edge, deck; mitcad#477), a wood screw or
  nail, a joist hanger, a post base, a round or square steel tube) for
  sheets, plates, shafts, screws, the timber parts (mitcad#478) and the
  steel tubes (mitcad#873) sold without a CAD file.

Mount frames are joint origins named mount:<name>: found from a profile's
section ({"auto": "profile"}) or listed. The supplier's files are read
from --source and never copied into the library: only the parametric
designs made from them are.

mitcad-cli does the modelling: `import` reads a source, and `mcp
--workspace` (the Model Context Protocol over stdin and stdout, without a
UI) runs the commands and queries the generator needs answers to (the
section's curves, the profile regions' keys).
"""

import argparse
import base64
import copy
import glob
import json
import math
import os
import random
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import library_previews as previews  # noqa: E402

LIBRARY_ID = "mitcad-parts"
LIBRARY_NAME = "Mitcad parts"
LIBRARY_DESCRIPTION = ("Mitcad's parts: ISO metric screws, nuts and washers, and slot profiles, connectors, nuts, "
                       "caps, panels and linear parts for the BUILD tab.")
LICENSE = "MIT"
PROTOCOL = "2025-11-25"
# Aluminium, kg per mm3: a profile's mass per metre when the group file
# gives none, and its density when neither the part nor its group file
# names one (`density`, kg/m3).
ALUMINIUM = 2.7e-6
MAKES = ("section", "solid", "model")
HERE = os.path.dirname(os.path.abspath(__file__))


class PartError(Exception):
    """A part that cannot be made: reported by its id and left out."""


def fail(message):
    """A broken group file or another reason to stop the whole run."""
    sys.exit(f"make-profile-library: {message}")


def mm(value):
    """An expression in millimetres: `8.5 mm`."""
    text = f"{value:.4f}".rstrip("0").rstrip(".")
    if text in ("-0", ""):
        text = "0"
    return f"{text} mm"


def length_of(value, what):
    """A length of a group file (a number in mm or text with a unit), in mm."""
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        text = value.strip()
        for unit, factor in (("mm", 1.0), ("cm", 10.0), ("m", 1000.0), ("in", 25.4)):
            if text.endswith(unit):
                try:
                    return float(text[: -len(unit)].strip().replace(",", ".")) * factor
                except ValueError:
                    break
        try:
            return float(text.replace(",", "."))
        except ValueError:
            pass
    raise PartError(f"{what} is no length: {value!r}")


def density_of(entry):
    """The part's density in kg per m3: its own `density`, else its group
    file's, else None. A number, or text with the unit kg/m3."""
    for value in (entry.get("density"), entry.get("_group_density")):
        if value is None:
            continue
        if isinstance(value, str):
            text = value.strip()
            if text.endswith("kg/m3"):
                text = text[: -len("kg/m3")]
            try:
                value = float(text.strip().replace(",", "."))
            except ValueError:
                fail(f"{entry['id']}: density {value!r} is no number of kg/m3")
        if not isinstance(value, (int, float)) or value <= 0:
            fail(f"{entry['id']}: density {value!r} is no number of kg/m3 above 0")
        return float(value)
    return None


def merged_block(base, over):
    """`over` merged into a copy of `base`: objects key by key, other
    values replaced (a configuration row's build block over its part's)."""
    out = copy.deepcopy(base)
    for key, value in over.items():
        if isinstance(value, dict) and isinstance(out.get(key), dict):
            out[key] = merged_block(out[key], value)
        else:
            out[key] = copy.deepcopy(value)
    return out


def run(command, cwd=None, check=True):
    result = subprocess.run([str(c) for c in command], cwd=cwd, capture_output=True, text=True,
                            encoding="utf-8", errors="replace")
    if check and result.returncode != 0:
        # One line: the exit status (a crash's code in hex, as Windows
        # gives it) and the last lines the program wrote.
        status = result.returncode
        code = f"{status} (0x{status & 0xFFFFFFFF:08X})" if status < 0 or status > 255 else str(status)
        lines = [line.strip() for line in f"{result.stdout}\n{result.stderr}".splitlines() if line.strip()]
        raise PartError(f"{' '.join(map(str, command))} failed with exit status {code}: "
                        f"{' | '.join(lines[-8:]) or 'no output'}")
    return result


def write(path, text):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


# ---------------------------------------------------------------------
# mitcad-cli mcp: a session of documents with commands and queries.

class Mcp:
    def __init__(self, cli, workspace):
        self.stderr = tempfile.TemporaryFile()
        self.process = subprocess.Popen([cli, "mcp", "--workspace", workspace], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.stderr)
        self.next_id = 0
        self.request("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                    "clientInfo": {"name": "make-profile-library", "version": "1"}})
        self.notify("notifications/initialized")

    def diagnostics(self):
        self.stderr.seek(0)
        return self.stderr.read().decode("utf-8", "replace")

    def send(self, message):
        self.process.stdin.write((json.dumps(message, separators=(",", ":")) + "\n").encode("utf-8"))
        self.process.stdin.flush()

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)

    def request(self, method, params):
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        while True:
            line = self.process.stdout.readline()
            if not line:
                fail("mitcad-cli mcp stopped:\n" + self.diagnostics())
            message = json.loads(line)
            if message.get("id") == self.next_id:
                break
        if "error" in message:
            raise PartError(f"{method}: {message['error'].get('message', message['error'])}")
        return message["result"]

    def tool(self, name, arguments):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        content = result.get("structuredContent")
        if content is None:
            content = json.loads(result["content"][0]["text"])
        if result.get("isError"):
            message = content.get("error") if isinstance(content, dict) else None
            if not message and isinstance(content, dict):
                failed = [f"{f.get('name')}: {f.get('error')}" for f in
                          content.get("timeline", {}).get("features", []) if f.get("status") == "error"]
                message = "; ".join(failed) or json.dumps(content)[:400]
            raise PartError(f"{name}: {message}")
        return content

    # A blank design in the workspace, as mitcad-cli makes one: its root
    # takes the features. MCP's document_create makes a new design as File
    # > New does (the root an assembly, the features going into a component
    # Component1), which a library's design must not be.
    BLANK = "blank.mitcad"

    def create(self):
        return self.tool("document_open", {"path": self.BLANK})["document"]

    def open(self, path):
        return self.tool("document_open", {"path": path})["document"]

    def close(self, document):
        try:
            self.tool("document_close", {"document": document, "discard": True})
        except PartError:
            pass

    def command(self, document, command):
        """Runs a command; its result, and the timeline after it."""
        content = self.tool("model_command", {"document": document, "command": command})
        return content.get("result"), content.get("timeline", {})

    def add(self, document, command):
        """Adds a feature; its uid."""
        result, timeline = self.command(document, command)
        if isinstance(result, dict) and result.get("uid"):
            return result["uid"]
        features = timeline.get("features") or []
        if not features:
            raise PartError(f"{command.get('cmd')} added no feature")
        return features[-1]["uid"]

    def query(self, document, query):
        return self.tool("model_query", {"document": document, "query": query})["result"]

    def save(self, document, path):
        self.tool("document_save", {"document": document, "path": path, "overwrite": True})

    def finish(self):
        try:
            self.process.stdin.close()
            self.process.wait(timeout=60)
        except (OSError, subprocess.TimeoutExpired):
            self.process.kill()


# ---------------------------------------------------------------------
# Geometry of sections: curves in the section's plane, centred.

def dot(a, b):
    return sum(a[i] * b[i] for i in range(len(a)))


def bspline_point(degree, control, weights, knots, t):
    """A point of a (rational) B-spline by de Boor's algorithm."""
    n = len(control)
    if not weights:
        weights = [1.0] * n
    if not knots:
        inner = max(n - degree - 1, 0)
        knots = [0.0] * (degree + 1) + [(i + 1) / (inner + 1) for i in range(inner)] + [1.0] * (degree + 1)
    span = degree
    while span < n - 1 and t >= knots[span + 1]:
        span += 1
    d = [[control[j][0] * weights[j], control[j][1] * weights[j], weights[j]]
         for j in range(span - degree, span + 1)]
    for r in range(1, degree + 1):
        for j in range(degree, r - 1, -1):
            i = span - degree + j
            denominator = knots[i + degree - r + 1] - knots[i]
            alpha = 0.0 if denominator == 0 else (t - knots[i]) / denominator
            d[j] = [(1 - alpha) * d[j - 1][k] + alpha * d[j][k] for k in range(3)]
    w = d[degree][2] or 1.0
    return (d[degree][0] / w, d[degree][1] / w)


def bspline_range(degree, control, knots):
    if not knots:
        return 0.0, 1.0
    return knots[degree], knots[len(control)]


def polyline(curve, samples=48):
    """Points along a curve, for bounds and previews."""
    kind = curve["type"]
    if kind == "line":
        return [curve["start"], curve["end"]]
    if kind in ("circle", "arc"):
        cx, cy = curve["center"]
        r = curve["radius"]
        if kind == "circle":
            a0, a1 = 0.0, 2 * math.pi
        else:
            a0 = math.atan2(curve["start"][1] - cy, curve["start"][0] - cx)
            a1 = math.atan2(curve["end"][1] - cy, curve["end"][0] - cx)
            while a1 <= a0:
                a1 += 2 * math.pi
        steps = max(4, int(samples * (a1 - a0) / (2 * math.pi)))
        return [(cx + r * math.cos(a0 + (a1 - a0) * i / steps), cy + r * math.sin(a0 + (a1 - a0) * i / steps))
                for i in range(steps + 1)]
    if kind == "spline":
        lo, hi = bspline_range(curve["degree"], curve["control"], curve["knots"])
        return [bspline_point(curve["degree"], curve["control"], curve["weights"], curve["knots"],
                              lo + (hi - lo) * i / samples) for i in range(samples + 1)]
    raise PartError(f"a {kind} in the section")


def bounds(curves):
    xs, ys = [], []
    for curve in curves:
        for x, y in polyline(curve):
            xs.append(x)
            ys.append(y)
    if not xs:
        raise PartError("the section has no curves")
    return min(xs), min(ys), max(xs), max(ys)


def transform_curves(curves, function, mirrored):
    """Curves with every point moved by `function`; arcs keep running
    counter-clockwise when the map mirrors."""
    out = []
    for curve in curves:
        c = dict(curve)
        for key in ("start", "end", "center"):
            if key in c:
                c[key] = function(c[key])
        if "control" in c:
            c["control"] = [function(p) for p in c["control"]]
        if c["type"] == "arc" and mirrored:
            c["start"], c["end"] = c["end"], c["start"]
        out.append(c)
    return out


def cross(a, b):
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]


def unit(vector, what):
    try:
        v = [float(c) for c in vector]
    except (TypeError, ValueError):
        v = []
    if len(v) != 3 or dot(v, v) < 1e-12:
        fail(f"{what} is no direction [x, y, z]")
    n = math.sqrt(dot(v, v))
    return [c / n for c in v]


def section_axes(entry, extents):
    """The part's x, y and z axes in the source's coordinates, and the
    source's axis the part runs along. `place` {"z", "x"} names them;
    without it the part runs along the source's longest extent (or
    `axis`), x and y along the next axes in turn."""
    place = entry.get("place") or {}
    if "z" in place:
        z = unit(place["z"], f"{entry['id']}: place.z")
        axis = max(range(3), key=lambda i: abs(z[i]))
        if abs(abs(z[axis]) - 1) > 1e-6:
            fail(f"{entry['id']}: place.z is not along a model axis")
        x = unit(place.get("x", [1 if i == (axis + 1) % 3 else 0 for i in range(3)]), f"{entry['id']}: place.x")
        if abs(dot(x, z)) > 1e-6:
            fail(f"{entry['id']}: place.x is not square to place.z")
        return axis, x, cross(z, x), z
    axes = {"x": 0, "y": 1, "z": 2}
    axis = axes.get(entry.get("axis", ""), max(range(3), key=lambda i: extents[i]))
    e = [[1.0 if i == j else 0.0 for i in range(3)] for j in range(3)]
    return axis, e[(axis + 1) % 3], e[(axis + 2) % 3], e[axis]


def section_curves(sketch, part_x, part_y):
    """The projected curves of a sketch on an end face, in the section's
    coordinates: along the part's x and y axes (in the source's
    coordinates), so that the part's axis is +z."""
    frame = sketch.get("frame")
    if not frame:
        raise PartError("the sketch on the end face did not evaluate")
    origin, xa, ya = frame["origin"], frame["x_axis"], frame["y_axis"]

    def mapped(p):
        point = [origin[i] + p[0] * xa[i] + p[1] * ya[i] for i in range(3)]
        return (dot(point, part_x), dot(point, part_y))

    mirrored = dot(xa, part_x) * dot(ya, part_y) - dot(ya, part_x) * dot(xa, part_y) < 0
    curves = []
    for entity in sketch.get("entities", []):
        kind = entity.get("type")
        geometry = entity.get("geometry")
        if kind == "point" or geometry is None or entity.get("construction"):
            continue
        if kind == "line":
            curves.append({"type": "line", "start": geometry["start"], "end": geometry["end"]})
        elif kind == "circle":
            curves.append({"type": "circle", "center": geometry["center"], "radius": geometry["radius"]})
        elif kind == "arc":
            curves.append({"type": "arc", "center": geometry["center"], "radius": geometry["radius"],
                           "start": geometry["start"], "end": geometry["end"]})
        elif kind == "spline":
            curves.append({"type": "spline", "degree": geometry["degree"], "control": geometry["control"],
                           "weights": geometry.get("weights") or [], "knots": geometry.get("knots") or []})
        else:
            raise PartError(f"the section has a {kind}, which the generator does not read")
    return transform_curves(curves, mapped, mirrored)


def centred(curves, place):
    """The curves with the centre of their bounds at the origin, then
    turned by place.rotate (degrees) and moved by place.shift; and that
    map of the section's points."""
    x0, y0, x1, y1 = bounds(curves)
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    angle = math.radians(float(place.get("rotate", 0)))
    dx, dy = [float(v) for v in place.get("shift", [0, 0])]
    c, s = math.cos(angle), math.sin(angle)

    def moved(p):
        x, y = p[0] - cx, p[1] - cy
        x, y = c * x - s * y + dx, s * x + c * y + dy
        return (round(x, 9) + 0.0, round(y, 9) + 0.0)

    return transform_curves(curves, moved, False), moved


def sketch_definition(curves):
    """A sketch on XY holding the curves as fixed geometry: ends shared
    where curves meet, so that the loops are closed."""
    entities = []
    ends = {}
    counter = [0]

    def new_id(prefix):
        counter[0] += 1
        return f"{prefix}{counter[0]}"

    def point(p, shared=True):
        key = (round(p[0], 6), round(p[1], 6))
        if shared and key in ends:
            return ends[key]
        uid = new_id("p")
        entities.append({"id": uid, "type": "point", "at": [p[0], p[1]], "fixed": True})
        if shared:
            ends[key] = uid
        return uid

    for curve in curves:
        kind = curve["type"]
        if kind == "line":
            a, b = point(curve["start"]), point(curve["end"])
            if a == b:
                continue
            entities.append({"id": new_id("c"), "type": "line", "start": a, "end": b, "fixed": True})
        elif kind == "circle":
            centre = point(curve["center"], shared=False)
            entities.append({"id": new_id("c"), "type": "circle", "center": centre, "radius": curve["radius"],
                             "fixed": True})
        elif kind == "arc":
            centre = point(curve["center"], shared=False)
            a, b = point(curve["start"]), point(curve["end"])
            entities.append({"id": new_id("c"), "type": "arc", "center": centre, "start": a, "end": b,
                             "fixed": True})
        elif kind == "spline":
            control = curve["control"]
            ids = [point(control[0])] + [point(p, shared=False) for p in control[1:-1]] + [point(control[-1])]
            spline = {"id": new_id("c"), "type": "spline", "degree": curve["degree"], "control": ids,
                      "fixed": True}
            if curve["weights"]:
                spline["weights"] = curve["weights"]
            if curve["knots"]:
                spline["knots"] = curve["knots"]
            entities.append(spline)
    return {"type": "sketch", "plane": "xy", "entities": entities}


# ---------------------------------------------------------------------
# Frames of a profile found from its section.

def profile_frames(curves, tolerance=1e-3):
    """The ends, the core bores and the slots of a centred section, named
    by the layout of docs/libraries.md: (name, kind, origin, z, x,
    extra)."""
    x0, y0, x1, y1 = bounds(curves)
    frames = []
    # Bores: circles inside the section, the centred one `core`, others
    # core_<n> from the lower coordinate.
    circles = [c for c in curves if c["type"] == "circle"]
    smaller = min(x1 - x0, y1 - y0) / 3
    bores = [c for c in circles if c["radius"] < smaller]
    centre = [c for c in bores if math.hypot(*c["center"]) < 0.05]
    if centre:
        bore = min(centre, key=lambda c: c["radius"])
        frames.append(("core", "bore", [0.0, 0.0, 0.0], [0, 0, -1], [1, 0, 0],
                       {"diameter": mm(2 * bore["radius"])}))
    else:
        others = sorted(bores, key=lambda c: (round(c["center"][1], 3), round(c["center"][0], 3)))
        for n, bore in enumerate(others, 1):
            frames.append((f"core_{n}", "bore", [bore["center"][0], bore["center"][1], 0.0], [0, 0, -1], [1, 0, 0],
                           {"diameter": mm(2 * bore["radius"])}))
    # Slots: gaps between the straight pieces of the outline on each side.
    openings = []
    lines = [c for c in curves if c["type"] == "line"]
    for side, along, at, normal in (("x+", 1, x1, [1, 0, 0]), ("x-", 1, x0, [-1, 0, 0]),
                                    ("y+", 0, y1, [0, 1, 0]), ("y-", 0, y0, [0, -1, 0])):
        across = 1 - along
        pieces = sorted((min(l["start"][along], l["end"][along]), max(l["start"][along], l["end"][along]))
                        for l in lines
                        if abs(l["start"][across] - at) < tolerance and abs(l["end"][across] - at) < tolerance)
        merged = []
        for lo, hi in pieces:
            if merged and lo <= merged[-1][1] + tolerance:
                merged[-1][1] = max(merged[-1][1], hi)
            else:
                merged.append([lo, hi])
        gaps = [(a[1], b[0]) for a, b in zip(merged, merged[1:]) if b[0] - a[1] > tolerance]
        for n, (lo, hi) in enumerate(gaps, 1):
            middle = (lo + hi) / 2
            origin = [at, middle, 0.0] if side[0] == "x" else [middle, at, 0.0]
            frames.append((f"slot_{side}_{n}", "slot", origin, normal, [0, 0, 1], {}))
            openings.append(hi - lo)
    return frames, openings


def listed_frames(entries, what):
    out = []
    for i, frame in enumerate(entries):
        if not isinstance(frame, dict) or not isinstance(frame.get("name"), str):
            fail(f"{what}: frames[{i}] has no name")
        name = frame["name"]
        extra = {k: frame[k] for k in ("diameter", "accepts") if k in frame}
        if name in ("end_a", "end_b") and "origin" not in frame:
            out.append((name, frame.get("kind", "end"), None, None, None, extra))
            continue
        try:
            origin = frame["origin"]
            if len(origin) != 3 or not all(isinstance(v, (int, float, str)) for v in origin):
                raise TypeError
            origin = [v if isinstance(v, str) else float(v) for v in origin]
            z = [float(v) for v in frame["z"]]
            x = [float(v) for v in frame.get("x", [1, 0, 0] if abs(z[2]) > 0.9 else [0, 0, 1])]
        except (KeyError, TypeError, ValueError):
            fail(f"{what}: frame {name}: origin is three numbers (or the z one an expression of the length), "
                 "z and x are three numbers")
        if "kind" not in frame:
            fail(f"{what}: frame {name} has no kind")
        # One coordinate may be an expression, the one along the frame's z
        # (a timber board's face frames follow its width and thickness).
        following = [i for i in range(3) if isinstance(origin[i], str)]
        if len(following) > 1:
            fail(f"{what}: frame {name}: only one coordinate of an origin may name a parameter")
        if following and abs(abs(z[following[0]]) - 1) > 1e-6:
            fail(f"{what}: frame {name}: an origin that follows a parameter has its z along "
                 f"+{'XYZ'[following[0]]} or -{'XYZ'[following[0]]}")
        out.append((name, frame["kind"], origin, z, x, extra))
    return out


def add_frames(mcp, document, frames, length):
    """Adds the frames as joint origins mount:<name>, in their order with
    end_a and end_b first. With a length parameter, end_a and end_b
    without an origin are XY turned over and a plane at the length. A
    frame whose origin's z is an expression (`[0, 0, "L"]`) lies on a plane
    offset by it along Z (from XY, or from a fixed plane through its x and
    y), so that it follows the parameter; the others are fixed planes."""
    def follow(name, label, origin, z, x):
        on_axis = abs(origin[0]) < 1e-9 and abs(origin[1]) < 1e-9 and dot(x, [1, 0, 0]) > 1 - 1e-9
        base = "xy" if on_axis else {"origin": [origin[0], origin[1], 0.0], "normal": [0, 0, 1], "x_axis": x}
        plane = mcp.add(document, {"cmd": "add_feature", "name": label, "def": {
            "type": "construction_plane", "definition": {"type": "offset", "plane": base, "distance": origin[2]}}})
        definition = {"type": "joint_origin", "geometry": plane}
        if z[2] < 0:
            definition["flip"] = True
        return definition

    def follow_side(name, origin, z, x):
        """A frame on a side whose x or y coordinate is an expression (a
        board's face at W / 2): on a plane offset by it from a fixed plane
        through the frame's other coordinates, facing the frame's z."""
        i = 0 if isinstance(origin[0], str) else 1
        expression = origin[i].strip()
        normal = [0.0, 0.0, 0.0]
        normal[i] = 1.0 if z[i] > 0 else -1.0
        if z[i] < 0:
            expression = expression[1:].strip() if expression.startswith("-") else f"-({expression})"
        base = [0.0 if j == i else float(origin[j]) for j in range(3)]
        plane = mcp.add(document, {"cmd": "add_feature", "name": f"Plane of {name}", "def": {
            "type": "construction_plane", "definition": {
                "type": "offset", "plane": {"origin": base, "normal": normal, "x_axis": x},
                "distance": expression}}})
        return {"type": "joint_origin", "geometry": plane}

    ordered = sorted(frames, key=lambda f: {"end_a": 0, "end_b": 1}.get(f[0], 2))
    for name, _, origin, z, x, _ in ordered:
        if origin is not None and (isinstance(origin[0], str) or isinstance(origin[1], str)):
            definition = follow_side(name, origin, z, x)
        elif name == "end_a" and length and origin is None:
            definition = {"type": "joint_origin", "geometry": "xy", "flip": True}
        elif name == "end_b" and length and origin is None:
            definition = follow(name, "End B plane", [0.0, 0.0, length], [0, 0, 1], [1, 0, 0])
        elif origin is None:
            raise PartError(f"frame {name} needs an origin (the part has no length)")
        elif isinstance(origin[2], str):
            label = "End B plane" if name == "end_b" else f"Plane of {name}"
            definition = follow(name, label, origin, z, x)
        else:
            definition = {"type": "joint_origin", "geometry": {"origin": origin, "normal": z, "x_axis": x}}
        mcp.add(document, {"cmd": "add_feature", "name": f"mount:{name}", "def": definition})


def set_frames(entry, build, made, listed):
    """The manifest's frames are the frames made in the design, keeping
    the extra fields (accepts, diameter) of those the group file's
    build.frames lists; a warning names the frames that build.frames lists
    and the design has not, or the reverse."""
    expected = [f.get("name") for f in build.get("frames") or [] if isinstance(f, dict)]
    names = [f[0] for f in made]
    dropped = [n for n in expected if n not in names]
    added = [n for n in names if n not in expected]
    if expected and (dropped or added):
        print(f"warning {entry['id']}: build.frames is the frames made: "
              + "; ".join(filter(None, [f"left out (not in the design) {', '.join(dropped)}" if dropped else "",
                                         f"added {', '.join(added)}" if added else ""])))
    build["frames"] = frame_entries(made, listed)


def frame_entries(frames, listed):
    """The manifest's frames: name, kind and extras, with what the group
    file lists for a frame of the same name (accepts, diameter)."""
    extras = {f["name"]: f for f in listed if isinstance(f, dict) and "name" in f}
    out = []
    for name, kind, _, _, _, extra in frames:
        entry = {"name": name, "kind": kind}
        entry.update(extra)
        for key, value in extras.get(name, {}).items():
            if key in ("accepts", "diameter", "kind"):
                entry[key] = value
        out.append(entry)
    return out


# ---------------------------------------------------------------------
# The three ways of making a part.

def import_source(ctx, entry):
    source = entry.get("source")
    if not source:
        raise PartError("no source file")
    path = os.path.join(ctx.source, source)
    if not os.path.isfile(path):
        raise PartError(f"the source {source} is not in --source")
    imported = f"source-{entry['id']}.mitcad"
    run([ctx.cli, "import", path, "--save", os.path.join(ctx.scratch, imported)])
    return imported


def main_body(mcp, document):
    bodies = [b for b in mcp.query(document, {"query": "bodies", "properties": True})
              if b.get("kind", "solid") == "solid"]
    if not bodies:
        raise PartError("the source has no solid")
    return max(bodies, key=lambda b: b.get("volume", 0))


def make_section(ctx, mcp, entry, build):
    """A profile or another cut-to-length part from its section."""
    source = mcp.open(import_source(ctx, entry))
    try:
        body = main_body(mcp, source)
        low, high = body["bbox"]["min"], body["bbox"]["max"]
        extents = [high[i] - low[i] for i in range(3)]
        axis, part_x, part_y, part_z = section_axes(entry, extents)
        source_length = extents[axis]
        ends, start = end_faces(mcp, source, body, part_z, extents, axis)
        # The section: every end face projected into one sketch on the
        # largest (they lie in one plane).
        sketch_uid = mcp.add(source, {"cmd": "sketch.create",
                                      "plane": {"face": ends[0]["names"][0], "body": body["uid"]}})
        for face in ends:
            mcp.command(source, {"cmd": "sketch.project", "sketch": sketch_uid, "source": face["names"][0],
                                 "body": body["uid"]})
        sketch = mcp.query(source, {"query": "sketch", "uid": sketch_uid})
        # A part sold with holes at a pitch (a rail) keeps them: those
        # nearest end A, found in the source.
        pitched = bool((build.get("length") or {}).get("pitch"))
        holes = source_holes(mcp, source, body, part_z, start) if pitched else []
    finally:
        mcp.close(source)
    face_areas = [f.get("area", 0) for f in ends]
    curves, moved = centred(section_curves(sketch, part_x, part_y), entry.get("place") or {})

    def to_part(point):
        x, y = moved((dot(point, part_x), dot(point, part_y)))
        return [x, y, dot(point, part_z) - start]

    for hole in holes:
        tip = to_part([hole["point"][i] + hole["direction"][i] for i in range(3)])
        hole["point"] = to_part(hole["point"])
        hole["direction"] = [tip[i] - hole["point"][i] for i in range(3)]
    source_area = body["volume"] / source_length
    return build_cut_part(ctx, mcp, entry, build, curves, face_areas, source_length, holes, source_area)


def source_holes(mcp, document, body, part_z, start):
    """The holes across a cut-to-length part nearest its end A: circular
    edges whose axes are square to the part's axis, grouped by the line
    they lie on. Each hole: its start point and direction into the
    material, its diameter, a counterbore's diameter and depth, and
    whether it goes through."""
    edges = mcp.query(document, {"query": "edges", "body": body["uid"]})
    stacks = {}
    for edge in edges:
        if edge.get("curve") != "circle" or not edge.get("name"):
            continue
        measured = mcp.query(document, {"query": "measure", "a": {"body": body["uid"], "edge": edge["name"]}})
        circle = (measured.get("a") or {}).get("circle")
        if not circle or not circle.get("axis") or not circle.get("center"):
            continue
        axis = circle["axis"]
        length = math.sqrt(dot(axis, axis))
        if length < 1e-12:
            continue
        axis = [a / length for a in axis]
        if abs(dot(axis, part_z)) > 1e-6:
            continue
        # One direction per line, so that the circles of a hole meet.
        k = max(range(3), key=lambda i: abs(axis[i]))
        if axis[k] < 0:
            axis = [-a for a in axis]
        center = circle["center"]
        across = cross(axis, part_z)
        key = (round(dot(center, part_z), 2), round(dot(center, across), 2), k)
        stacks.setdefault(key, []).append((dot(center, axis), float(circle["radius"]), center, axis))
    if not stacks:
        return []
    low, high = body["bbox"]["min"], body["bbox"]["max"]
    corners = [[(low, high)[(n >> i) & 1][i] for i in range(3)] for n in range(8)]
    nearest = min(key[0] for key in stacks)
    holes = []
    for key, stack in stacks.items():
        if key[0] - nearest > 0.5:
            continue
        stack.sort(key=lambda c: c[0])
        axis = stack[0][3]
        lo, hi = stack[0][0], stack[-1][0]
        small = min(c[1] for c in stack)
        large = max(c[1] for c in stack)
        counterbore = None
        if large - small > 1e-3 and abs(stack[0][1] - large) < 1e-3 and abs(stack[-1][1] - large) > 1e-3:
            # The counterbore at the lower end: the hole goes along +axis.
            begin, direction = lo, axis
            floor = max(c[0] for c in stack if abs(c[1] - large) < 1e-3)
            counterbore = (2 * large, floor - lo)
        else:
            begin, direction = hi, [-a for a in axis]
            if large - small > 1e-3:
                floor = min(c[0] for c in stack if abs(c[1] - large) < 1e-3)
                counterbore = (2 * large, hi - floor)
        # A counterbore without depth (a chamfer's or countersink's edge at
        # the start) is no counterbore: a plain hole.
        if counterbore and counterbore[1] < 1e-3:
            counterbore = None
        extent_lo = min(dot(c, axis) for c in corners)
        extent_hi = max(dot(c, axis) for c in corners)
        through = abs(lo - extent_lo) < 0.05 and abs(hi - extent_hi) < 0.05
        center = stack[0][2]
        point = [center[i] + (begin - stack[0][0]) * axis[i] for i in range(3)]
        holes.append({"point": point, "direction": direction, "diameter": 2 * small,
                      "counterbore": counterbore, "through": through, "depth": hi - lo,
                      "at": dot(point, part_z) - start})
    return holes


def add_holes(mcp, document, entry, holes, parameter, pitch):
    """The holes of a part with a pitch: each at `first` from end A, on a
    plane at its start facing out of the material, then a pattern along
    the axis at `distance` whose count follows the length, so that a
    shorter part keeps the holes that fit."""
    first = length_of(pitch.get("first", 0), "build.length.pitch.first")
    distance = length_of(pitch.get("distance"), "build.length.pitch.distance")
    if distance <= 0:
        raise PartError("build.length.pitch.distance is not above 0")
    uids = []
    for n, hole in enumerate(holes, 1):
        if abs(hole["at"] - first) > 0.5:
            print(f"warning {entry['id']}: the source's hole nearest end A is {hole['at']:.2f} mm from it, "
                  f"the pitch's first {first:.2f} mm; the design puts it at the pitch's")
        x, y, _ = hole["point"]
        normal = [-c for c in hole["direction"]]
        plane = mcp.add(document, {"cmd": "add_feature", "name": f"Hole plane {n}", "def": {
            "type": "construction_plane", "definition": {
                "type": "fixed", "origin": [x, y, first], "x_axis": [0, 0, 1], "y_axis": cross(normal, [0, 0, 1])}}})
        definition = {"type": "hole", "placement": {"type": "plane", "plane": plane, "points": [[x, y, first]]},
                      "diameter": round(hole["diameter"], 6)}
        if hole["counterbore"] and hole["counterbore"][1] > 1e-3 and hole["counterbore"][0] > hole["diameter"]:
            definition["kind"] = {"type": "counterbore", "diameter": round(hole["counterbore"][0], 6),
                                  "depth": round(hole["counterbore"][1], 6)}
        definition["extent"] = ({"type": "through_all"} if hole["through"] or hole["depth"] < 1e-3
                                else {"type": "distance", "depth": round(hole["depth"], 6)})
        uids.append(mcp.add(document, {"cmd": "add_feature", "name": f"Hole {n}", "def": definition}))
    count = f"max(1, floor(({parameter} - {mm(first)}) / ({mm(distance)})) + 1)"
    mcp.add(document, {"cmd": "add_feature", "name": "Holes at the pitch", "def": {
        "type": "rectangular_pattern", "objects": {"type": "features", "features": uids},
        "direction1": {"axis": "z", "quantity": count, "distance": mm(distance)},
        "distance_type": "spacing"}})


def end_faces(mcp, document, body, part_z, extents, axis):
    """The planar faces at the end the part's z starts from, facing along
    -z, largest first, and where that end lies along z: those no larger than the section's bounds are
    tried, a sketch on each telling where it lies and which way it faces."""
    low, high = body["bbox"]["min"], body["bbox"]["max"]
    corners = [[(low, high)[(n >> i) & 1][i] for i in range(3)] for n in range(8)]
    start = min(dot(c, part_z) for c in corners)
    bounds_area = 1.0
    for i in range(3):
        if i != axis:
            bounds_area *= extents[i]
    faces = mcp.query(document, {"query": "faces", "body": body["uid"]})
    planar = [f for f in faces if f.get("surface") == "plane" and f.get("names")]
    candidates = sorted((f for f in planar if f.get("area", 0) <= bounds_area * 1.0001),
                        key=lambda f: -f.get("area", 0))
    tolerance = max(1e-3, 1e-6 * extents[axis])
    facing, seen = [], []
    for face in candidates:
        uid = mcp.add(document, {"cmd": "sketch.create", "plane": {"face": face["names"][0], "body": body["uid"]}})
        frame = mcp.query(document, {"query": "sketch", "uid": uid}).get("frame") or {}
        normal, origin = frame.get("normal") or [0, 0, 0], frame.get("origin") or [0, 0, 0]
        seen.append(f"{face.get('area', 0):.2f} mm2 at {dot(origin, part_z) - start:.3f}")
        if dot(normal, part_z) < -0.999:
            facing.append((dot(origin, part_z), face))
    # The faces facing out of the start that lie lowest along z: the end,
    # also when the bounds are a little larger than the body.
    first = min((at for at, _ in facing), default=None)
    ends = [] if first is None or first - start > max(0.5, 0.01 * extents[axis]) else [
        face for at, face in facing if at - first < tolerance]
    if not ends:
        raise PartError(f"no planar face at the start of its axis ({'xyz'[axis]}) facing out of it: of "
                        f"{len(planar)} planar faces {len(candidates)} fit the section's bounds "
                        f"({bounds_area:.2f} mm2), their areas and distances from that end: "
                        f"{', '.join(seen[:12]) or 'none'}")
    return ends, first


def build_cut_part(ctx, mcp, entry, build, curves, face_areas, source_length, holes, source_area):
    area = sum(face_areas)
    length_block = build.setdefault("length", {})
    parameter = length_block.setdefault("parameter", "L")
    default = length_of(length_block.get("default", source_length), "build.length.default")
    document = mcp.create()
    try:
        mcp.command(document, {"cmd": "add_parameter", "name": parameter, "expression": mm(default),
                               "comment": "Length"})
        sketch = mcp.add(document, {"cmd": "add_feature", "name": "Section", "def": sketch_definition(curves)})
        regions = mcp.query(document, {"query": "sketch", "uid": sketch}).get("regions") or []
        if not regions:
            raise PartError("the section's curves bound no region")
        # One region per end face: the one of the face's area.
        chosen = []
        for face_area in face_areas:
            left = [r for r in regions if r.get("key") not in chosen]
            region = min(left, key=lambda r: abs(r.get("area", 0) - face_area)) if left else None
            if region is None or abs(region.get("area", 0) - face_area) > 0.02 * face_area:
                raise PartError(f"no region of the section has the end face's area ({face_area:.2f} mm2; "
                                f"regions {[round(r.get('area', 0), 2) for r in regions]})")
            chosen.append(region["key"])
        mcp.add(document, {"cmd": "add_feature", "name": "Profile", "def": {
            "type": "extrude", "profiles": [{"sketch": sketch, "region": key} for key in chosen],
            "extent": {"type": "distance", "distance": parameter}, "operation": "new_body"}})
        bodies = mcp.query(document, {"query": "bodies", "properties": True})
        volume = sum(b.get("volume", 0) for b in bodies)
        if not bodies or abs(volume - area * default) > 0.01 * area * default:
            raise PartError(f"the profile's body is {volume:.1f} mm3, not {area * default:.1f}")
        pitch = length_block.get("pitch")
        if pitch and holes:
            solid = volume
            before = {f["uid"] for f in mcp.query(document, {"query": "timeline"}).get("features", [])}
            try:
                add_holes(mcp, document, entry, holes, parameter, pitch)
                bodies = mcp.query(document, {"query": "bodies", "properties": True})
                volume = sum(b.get("volume", 0) for b in bodies)
                if volume > solid * (1 - 1e-6):
                    raise PartError("the holes cut nothing")
            except PartError as e:
                # Holes that cannot be placed from the source: the part
                # without them, rather than no part.
                added = [f["uid"] for f in mcp.query(document, {"query": "timeline"}).get("features", [])
                         if f["uid"] not in before]
                for uid in reversed(added):
                    try:
                        mcp.command(document, {"cmd": "delete_feature", "uid": uid, "dependents": True})
                    except PartError:
                        pass
                reason = " | ".join(line.strip() for line in str(e).splitlines() if line.strip())
                print(f"warning {entry['id']}: its source's holes could not be placed ({reason}); it is made "
                      "without them")
                bodies = mcp.query(document, {"query": "bodies", "properties": True})
                volume = sum(b.get("volume", 0) for b in bodies)
        elif pitch:
            print(f"warning {entry['id']}: build.length.pitch gives holes, but the source has no holes across "
                  "the part; it is made without them")
        # The design against the source, per length: what differs (holes
        # the design does not have, machined ends) is said.
        per_length = volume / default
        if abs(per_length - source_area) > 0.05 * source_area:
            print(f"warning {entry['id']}: the design's volume per length is {per_length:.2f} mm2, the "
                  f"source's {source_area:.2f} mm2")
        frames = entry.get("frames") or {"auto": "profile"}
        openings = []
        if isinstance(frames, dict):
            if frames.get("auto") != "profile":
                fail(f"{entry['id']}: frames.auto is 'profile' or the frames are a list")
            found, openings = profile_frames(curves)
            found = [("end_a", "end", None, None, None, {}), ("end_b", "end", None, None, None, {})] + found
            found += listed_frames(frames.get("add", []), entry["id"])
            listed = (build.get("frames") or []) + frames.get("add", [])
        else:
            found = listed_frames(frames, entry["id"])
            listed = (build.get("frames") or []) + frames
        add_frames(mcp, document, found, parameter)
        set_frames(entry, build, found, listed)
        if build.get("role") == "profile":
            x0, y0, x1, y1 = bounds(curves)
            build.setdefault("section", {"width": mm(x1 - x0), "height": mm(y1 - y0)})
            if openings and "opening" not in build.get("slot", {}):
                build.setdefault("slot", {})["opening"] = mm(min(openings))
            density = density_of(entry)
            per_mm3 = ALUMINIUM if density is None else density * 1e-9
            build.setdefault("mass_per_metre", f"{per_length * 1000 * per_mm3:.3f} kg")
        finish_design(mcp, document, entry)
        return save(ctx, mcp, document, entry), {"kind": "section", "curves": curves}
    finally:
        mcp.close(document)


def placement_matrix(place, what):
    """A move's matrix from `place`: `matrix`; or `z` and `x` (the source's
    directions that become the part's z and x axes); or `rotate`
    ({"axis": "x" or [x, y, z], "angle": degrees}); then `translate` [x, y,
    z]."""
    if "matrix" in place:
        return [[float(v) for v in row] for row in place["matrix"]]
    rotation = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
    if "z" in place:
        z = unit(place["z"], f"{what}: place.z")
        x = unit(place.get("x", [1, 0, 0] if abs(z[0]) < 0.9 else [0, 0, 1]), f"{what}: place.x")
        if abs(dot(x, z)) > 1e-6:
            fail(f"{what}: place.x is not square to place.z")
        # The part's coordinates of a source point are its components along
        # the part's axes: the rows of the rotation.
        rotation = [x, cross(z, x), z]
    elif "rotate" in place:
        axis = place["rotate"].get("axis", "z")
        axis = {"x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}.get(axis, axis)
        norm = math.sqrt(dot(axis, axis))
        x, y, z = [float(v) / norm for v in axis]
        a = math.radians(float(place["rotate"].get("angle", 0)))
        c, s, t = math.cos(a), math.sin(a), 1 - math.cos(a)
        rotation = [[t * x * x + c, t * x * y - s * z, t * x * z + s * y],
                    [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
                    [t * x * z - s * y, t * y * z + s * x, t * z * z + c]]
    shift = [float(v) for v in place.get("translate", [0, 0, 0])]
    return [rotation[i] + [shift[i]] for i in range(3)]


def make_solid(ctx, mcp, entry, build):
    """A connector, nut, cap or other part from the source's solids."""
    document = mcp.open(import_source(ctx, entry))
    try:
        bodies = mcp.query(document, {"query": "bodies", "properties": True})
        if not bodies:
            raise PartError("the source has no bodies")
        place = entry.get("place") or {}
        if place:
            mcp.add(document, {"cmd": "add_feature", "name": "Placed", "def": {
                "type": "move", "bodies": [b["uid"] for b in bodies],
                "transform": {"type": "free", "matrix": placement_matrix(place, entry["id"])}}})
        frames = listed_frames(entry.get("frames") or [], entry["id"])
        length = (build.get("length") or {}).get("parameter")
        add_frames(mcp, document, frames, length)
        set_frames(entry, build, frames, build.get("frames") or [])
        # The source's colours are not Mitcad's appearances; a fastener's
        # finish (mitcad#526) is.
        solid_screw_thread(mcp, document, entry)
        set_appearance(mcp, document, entry)
        finish_design(mcp, document, entry)
        return save(ctx, mcp, document, entry), {"kind": "solid"}
    finally:
        mcp.close(document)


# ---------------------------------------------------------------------
# Screws' threads and heads (mitcad#526): a cosmetic thread on the shank,
# which the `threads` query lists and the preview models, and a wood
# screw's head edge rounded. A feature that cannot be made is left out
# with a warning; the part is made without it.

def try_feature(mcp, document, entry, command):
    """Adds a feature; its uid, or None (and no feature) when it fails."""
    try:
        uid = mcp.add(document, command)
    except PartError as e:
        uid, error = None, str(e)
    else:
        error = None
    features = mcp.query(document, {"query": "timeline"}).get("features") or []
    mine = [f for f in features if f.get("uid") == uid] if uid else \
        [f for f in features[-1:] if f.get("name") == command.get("name")]
    failed = [f for f in mine if f.get("status") == "error"]
    if failed or error:
        for feature in mine:
            mcp.command(document, {"cmd": "delete_feature", "uid": feature["uid"]})
        reason = error or failed[0].get("error")
        previews.warn(f"{entry['id']}: no {command.get('name', 'feature').lower()} ({reason})")
        return None
    return uid


def keep_thread(entry, command):
    """Records a cosmetic thread for the preview to model."""
    entry.setdefault("_threads", []).append(command)


def wood_screw_dress(mcp, document, entry, shank, head_uid, head, dk, length, d, diameter):
    """A wood screw's head edge rounded (the head's outer face's edge: the
    head cylinder's base, at z = 0 for a countersunk head and at -K for
    the others) and a cosmetic thread: the nearest forming screw thread
    (ISO 1478), a stand-in for the wood screw's own, over 60 % of the
    length from the tip (a pan head's connector screw to the head),
    chamfered at the tip."""
    body = mcp.query(document, {"query": "bodies"})[0]["uid"]
    try_feature(mcp, document, entry, {"cmd": "add_feature", "name": "Head rounding", "def": {
        "type": "fillet", "body": body, "edges": [f"E{{{head_uid}:bottom|{head_uid}:side0}}"],
        "radius": f"0.08 * {dk}"}})
    # The tip's chamfer follows a row's diameter (the thread's designation
    # is the default row's): about a forming thread's minor diameter.
    command = previews.thread_command([{"body": body, "face": f"{shank}:side0"}], previews.forming_thread(diameter),
                                      None if head == "pan" else f"0.6 * {length}",
                                      chamfer_diameter=f"0.58 * {d}")
    if try_feature(mcp, document, entry, command):
        keep_thread(entry, command)


def solid_screw_thread(mcp, document, entry):
    """A screw made from the supplier's solid (a hammer head screw, a
    central or self-forming screw): a cosmetic ISO metric thread of the
    size its name gives (`M8x20`; a self-forming `S12x30` the metric
    coarse thread of its diameter as a stand-in) on its cylindrical faces
    of that diameter. A part whose solid has its thread modelled (`"thread":
    "source"`: a shank of thread rings, no plain cylinder) keeps that one
    (mitcad#901)."""
    category = str(entry.get("category") or "").lower()
    if "screw" not in category and "screw" not in entry["name"].lower():
        return
    if entry.get("thread") == "source":
        print(f"{entry['id']}: the thread is the source's own", flush=True)
        return
    found = re.search(r"\b[MS](\d+(?:\.\d+)?)x\d", entry["name"])
    if not found:
        return
    diameter = float(found.group(1))
    faces = []
    for body in mcp.query(document, {"query": "bodies"}):
        for face in mcp.query(document, {"query": "faces", "body": body["uid"]}) or []:
            names = face.get("names") or []
            if face.get("surface") != "cylinder" or not names:
                continue
            try:
                measured = mcp.query(document, {"query": "measure", "a": {"body": body["uid"], "face": names[0]}})
            except PartError:
                continue
            radius = ((measured or {}).get("a") or {}).get("circle", {}).get("radius")
            if radius is not None and abs(2 * radius - diameter) < 0.3:
                faces.append({"body": body["uid"], "face": names[0]})
    if not faces:
        previews.warn(f"{entry['id']}: no shank of {diameter:g} mm for its thread")
        return
    command = previews.thread_command(faces, previews.metric_thread(diameter))
    if try_feature(mcp, document, entry, command):
        keep_thread(entry, command)


# ---------------------------------------------------------------------
# Timber parts (mitcad#478): boards, wood fasteners, sheets and the steel
# connectors of timber, modelled from their dimensions.

def value_of(value, parameters):
    """A size in mm: a number, a text with a unit, or a parameter's name."""
    if isinstance(value, str) and value.strip() in parameters:
        value = parameters[value.strip()]
    return length_of(value, "a size")


def board_sketch(w, t, width, thickness):
    """A rectangle `w` x `t` (parameter names) centred on the origin of XY,
    the width along X: its corners dimensioned by the parameters, its
    diagonal's middle on a fixed point at the origin."""
    hw, ht = width / 2, thickness / 2
    entities = [
        {"id": "p1", "type": "point", "at": [-hw, -ht]},
        {"id": "p2", "type": "point", "at": [hw, -ht]},
        {"id": "p3", "type": "point", "at": [hw, ht]},
        {"id": "p4", "type": "point", "at": [-hw, ht]},
        {"id": "p5", "type": "point", "at": [0, 0], "fixed": True},
        {"id": "c6", "type": "line", "start": "p1", "end": "p2"},
        {"id": "c7", "type": "line", "start": "p2", "end": "p3"},
        {"id": "c8", "type": "line", "start": "p3", "end": "p4"},
        {"id": "c9", "type": "line", "start": "p4", "end": "p1"},
        {"id": "c10", "type": "line", "start": "p1", "end": "p3", "construction": True},
    ]
    constraints = [
        {"id": "k11", "type": "horizontal", "line": "c6"},
        {"id": "k12", "type": "vertical", "line": "c7"},
        {"id": "k13", "type": "horizontal", "line": "c8"},
        {"id": "k14", "type": "vertical", "line": "c9"},
        {"id": "k15", "type": "midpoint", "point": "p5", "curve": "c10"},
    ]
    dimensions = [
        {"id": "k16", "type": "length", "line": "c6", "value": w},
        {"id": "k17", "type": "length", "line": "c7", "value": t},
    ]
    return {"type": "sketch", "plane": "xy", "entities": entities, "constraints": constraints,
            "dimensions": dimensions}


def board_frames(w, t):
    """A board's frames: its ends, and a face frame on each side at z = 0 on
    the side's centre line, z out of the side and x along +Z (as a profile's
    slot frames), following the width `w` and thickness `t`."""
    return [
        {"name": "end_a", "kind": "end"},
        {"name": "end_b", "kind": "end"},
        {"name": "face_xp", "kind": "face", "origin": [f"{w} / 2", 0, 0], "z": [1, 0, 0], "x": [0, 0, 1]},
        {"name": "face_xn", "kind": "face", "origin": [f"-{w} / 2", 0, 0], "z": [-1, 0, 0], "x": [0, 0, 1]},
        {"name": "face_yp", "kind": "face", "origin": [0, f"{t} / 2", 0], "z": [0, 1, 0], "x": [0, 0, 1]},
        {"name": "face_yn", "kind": "face", "origin": [0, f"-{t} / 2", 0], "z": [0, -1, 0], "x": [0, 0, 1]},
    ]


# Profiled boards (mitcad#477): Mitcad's own parametric sections, each a
# loop of points (and a deck board's rounded corners) whose coordinates
# are expressions of the board's parameters, so that a row's values (or a
# changed tongue, overlap or radius) reshape the section. The section lies
# as a board's does: its box centred on the origin, the width along X,
# the visible face towards +Y; a board that joins the next has its tongue
# (the edge that goes under the next board) towards +X and its groove
# towards -X, the cover width apart (docs/libraries.md, "Timber parts").
#
# Each profile: its parameters with their defaults (mm, or an angle in
# degrees), the parameters derived from them (expressions in the design,
# not in the configuration table), and the expressions of its cover width,
# its box's half width and half height.
BOARD_PROFILES = {
    "tongue_and_groove": {
        "parameters": {"TL": 8, "TT": 8, "TO": 0, "GC": 1, "GW": 0.5},
        "cover": "W - TL", "half": ("W / 2", "T / 2"),
    },
    "shiplap": {
        "parameters": {"LW": 12, "LT": 9},
        "cover": "W - LW", "half": ("W / 2", "T / 2"),
    },
    "feather_edge": {
        "parameters": {"TN": 6, "O": 30},
        # The board lies as it is laid: turned by FA so that its thick
        # edge rests on the face of the board under it, its thin edge on
        # the wall, each the cover width COV above the last.
        "derived": [("FA", "atan2(T - (T - TN) * (W - O) / W, W - O)"),
                    ("COV", "sqrt((W - O) * (W - O) + (T - (T - TN) * (W - O) / W) * (T - (T - TN) * (W - O) / W))"),
                    ("HX", "(W * cos(FA) + TN * sin(FA)) / 2"),
                    ("HY", "(T * cos(FA) + W * sin(FA)) / 2"),
                    ("YC", "(T * cos(FA) - W * sin(FA)) / 2")],
        "cover": "COV", "half": ("HX", "HY"),
    },
    "deck": {
        "parameters": {"R": 5},
        "cover": "W", "half": ("W / 2", "T / 2"),
    },
}
# The extra parameters of a variant: a V-joint's chamfer on the face edges
# of tongue-and-groove panelling (its width VB on the face, its angle VA to
# the face), a deck board's grooved face (the grooves' width and depth;
# `grooves` their count, fixed in the design).
BOARD_VARIANTS = {"v_joint": {"VB": 2, "VA": "45 deg"}, "grooves": {"GG": 5, "GD": 3}}
SAFE_MATH = {"sin": math.sin, "cos": math.cos, "tan": math.tan, "atan2": math.atan2, "sqrt": math.sqrt,
             "__builtins__": {}}


def angle_value(value):
    """An angle parameter's value: `"45 deg"` -> (expression, radians), or
    None for a length."""
    if isinstance(value, str) and value.strip().endswith("deg"):
        degrees = float(value.strip()[:-3].strip().replace(",", "."))
        return f"{degrees:g} deg", math.radians(degrees)
    return None


def board_parameters(dimensions):
    """A profiled board's parameters and their defaults: its profile's,
    its variant's and the `parameters` of dimensions over them (W, T, L
    and any of the profile's)."""
    profile = dimensions["profile"]
    out = dict(BOARD_PROFILES[profile]["parameters"])
    if dimensions.get("v_joint"):
        out.update(BOARD_VARIANTS["v_joint"])
    if dimensions.get("grooves"):
        out.update(BOARD_VARIANTS["grooves"])
    out.update(dimensions.get("parameters") or {})
    return out


def board_numbers(dimensions, values):
    """The board's parameters as numbers (mm; angles in radians), the
    derived ones evaluated by their expressions, for its area and box."""
    env = {}
    for name, value in values.items():
        angle = angle_value(value)
        env[name] = angle[1] if angle else length_of(value, f"parameter {name}")
    for name, expression in BOARD_PROFILES[dimensions["profile"]].get("derived", []):
        env[name] = eval(expression, dict(SAFE_MATH), dict(env))
    return env


def board_loop(dimensions):
    """The section's loop, counter-clockwise: ("point", x, y) and ("arc", cx,
    cy, x, y) (an arc on to x, y about cx, cy), each coordinate an
    expression of the parameters."""
    profile = dimensions["profile"]
    loop = []

    def point(x, y):
        loop.append(("point", x, y))

    if profile == "tongue_and_groove":
        x0, x1, x2 = "-W / 2", "W / 2 - TL", "W / 2"
        gx = "-W / 2 + TL + GC"
        yb, yt = "-T / 2", "T / 2"
        tb, tt = "TO - TT / 2", "TO + TT / 2"
        gb, gt = "TO - TT / 2 - GW / 2", "TO + TT / 2 + GW / 2"
        point(x0, yb)
        point(x1, yb)
        point(x1, tb)
        point(x2, tb)
        point(x2, tt)
        point(x1, tt)
        if dimensions.get("v_joint"):
            point(x1, "T / 2 - VB * tan(VA)")
            point("W / 2 - TL - VB", yt)
            point("-W / 2 + VB", yt)
            point(x0, "T / 2 - VB * tan(VA)")
        else:
            point(x1, yt)
            point(x0, yt)
        point(x0, gt)
        point(gx, gt)
        point(gx, gb)
        point(x0, gb)
    elif profile == "shiplap":
        lip = "-T / 2 + LT"
        point("-W / 2 + LW", "-T / 2")
        point("W / 2", "-T / 2")
        point("W / 2", lip)
        point("W / 2 - LW", lip)
        point("W / 2 - LW", "T / 2")
        point("-W / 2", "T / 2")
        point("-W / 2", lip)
        point("-W / 2 + LW", lip)
    elif profile == "feather_edge":
        # Its back from the thick edge (at the box's -X) to the thin one,
        # then its face back: the corners turned by FA, the box centred.
        point("-HX", "-YC")
        point("W * cos(FA) - HX", "-W * sin(FA) - YC")
        point("W * cos(FA) + TN * sin(FA) - HX", "-W * sin(FA) + TN * cos(FA) - YC")
        point("T * sin(FA) - HX", "T * cos(FA) - YC")
    elif profile == "deck":
        # From the bottom's left end, reached by the bottom left corner's
        # arc from the left side's lower end (the loop's last point).
        loop.append(("arc", "-W / 2 + R", "-T / 2 + R", "-W / 2 + R", "-T / 2"))
        point("W / 2 - R", "-T / 2")
        loop.append(("arc", "W / 2 - R", "-T / 2 + R", "W / 2", "-T / 2 + R"))
        point("W / 2", "T / 2 - R")
        loop.append(("arc", "W / 2 - R", "T / 2 - R", "W / 2 - R", "T / 2"))
        n = int(dimensions.get("grooves") or 0)
        for i in reversed(range(n)):
            # Groove i's middle at (i + 1) / (n + 1) of the width.
            middle = f"-W / 2 + {i + 1} * W / {n + 1}"
            point(f"{middle} + GG / 2", "T / 2")
            point(f"{middle} + GG / 2", "T / 2 - GD")
            point(f"{middle} - GG / 2", "T / 2 - GD")
            point(f"{middle} - GG / 2", "T / 2")
        point("-W / 2 + R", "T / 2")
        loop.append(("arc", "-W / 2 + R", "T / 2 - R", "-W / 2", "T / 2 - R"))
        point("-W / 2", "-T / 2 + R")
    return loop


def board_shape(dimensions, env):
    """The section's area (mm²) and its box's width and height at the
    numbers `env`: the loop's polygon and its arcs' segments (each arc
    counter-clockwise, from the point before it)."""
    def value(expression):
        return eval(expression, dict(SAFE_MATH), dict(env))

    loop = board_loop(dimensions)
    points = [(value(item[-2]), value(item[-1])) for item in loop]
    area = 0.0
    for i, item in enumerate(loop):
        (ax, ay), (bx, by) = points[i - 1], points[i]
        area += (ax * by - bx * ay) / 2
        if item[0] == "arc":
            cx, cy = value(item[1]), value(item[2])
            radius = math.hypot(ax - cx, ay - cy)
            sweep = (math.atan2(by - cy, bx - cx) - math.atan2(ay - cy, ax - cx)) % (2 * math.pi)
            area += radius * radius / 2 * (sweep - math.sin(sweep))
    half_w, half_h = (value(e) for e in BOARD_PROFILES[dimensions["profile"]]["half"])
    return area, 2 * half_w, 2 * half_h


def board_cover(dimensions, env):
    return eval(BOARD_PROFILES[dimensions["profile"]]["cover"], dict(SAFE_MATH), dict(env))


def board_block(dimensions, env):
    """A profiled board's `board` block at the numbers `env` (a row's):
    its profile, cover width, area and the sizes of its joint."""
    profile = dimensions["profile"]
    area, _, _ = board_shape(dimensions, env)
    block = {"profile": profile, "cover": mm(board_cover(dimensions, env)), "area": round(area, 3)}
    if profile == "tongue_and_groove":
        block["tongue"] = {"length": mm(env["TL"]), "thickness": mm(env["TT"])}
    elif profile == "shiplap":
        block["overlap"] = mm(env["LW"])
    elif profile == "feather_edge":
        block["overlap"] = mm(env["O"])
    else:
        block["edge_radius"] = mm(env["R"])
        block["grooves"] = int(dimensions.get("grooves") or 0)
    return block


def check_board(dimensions, env, what):
    """Refuses a profiled board's sizes that make no section: its joint
    wider or thicker than the board, a radius over half its thickness."""
    w, t = env["W"], env["T"]
    profile = dimensions["profile"]
    problems = []
    if profile == "tongue_and_groove":
        if not 0 < env["TL"] < w / 2:
            problems.append(f"the tongue's length {env['TL']:g} is not between 0 and half the width")
        if not (0 < env["TT"] and abs(env["TO"]) + env["TT"] / 2 + env["GW"] / 2 < t / 2):
            problems.append(f"the groove ({env['TT']:g} + {env['GW']:g} at {env['TO']:g}) is not inside "
                            f"the thickness {t:g}")
        if dimensions.get("v_joint") and env["VB"] * math.tan(env["VA"]) >= (
                t / 2 - env["TO"] - env["TT"] / 2 - env["GW"] / 2):
            problems.append("the V-joint's chamfer reaches the tongue")
    elif profile == "shiplap":
        if not (0 < env["LW"] < w / 2 and 0 < env["LT"] < t):
            problems.append(f"the overlap {env['LW']:g} x {env['LT']:g} does not fit a {t:g} x {w:g} board")
    elif profile == "feather_edge":
        if not (0 < env["TN"] < t and 0 < env["O"] < w / 2):
            problems.append(f"the thin edge {env['TN']:g} or the overlap {env['O']:g} does not fit "
                            f"a {t:g} x {w:g} board")
    else:
        n = int(dimensions.get("grooves") or 0)
        if not 0 < env["R"] < t / 2:
            problems.append(f"the edge radius {env['R']:g} is not between 0 and half the thickness")
        if n and not (0 < env["GD"] < t - env["R"] and env["GG"] < w / (n + 1)
                      and w / (n + 1) - env["GG"] / 2 > env["R"]):
            problems.append(f"{n} grooves {env['GG']:g} x {env['GD']:g} do not fit a {t:g} x {w:g} board")
    if problems:
        fail(f"{what}: {'; '.join(problems)}")


def profiled_sketch(dimensions, env):
    """The section's sketch: a fixed point at the origin and every point of
    the loop (and every arc's centre) dimensioned from it by signed
    horizontal and vertical distances, the parameters' expressions; each
    point joined to the one before it by a line or its arc."""
    return points_sketch([board_loop(dimensions)], env)


def points_sketch(loops, env):
    """A sketch of loops of points and arcs (board_loop's items) as a
    profiled board's: each point and arc centre dimensioned from a fixed
    point at the origin by expressions, evaluated at the numbers `env`."""
    entities, dimensions_out = [], []
    counter = [0]

    def new_id(prefix):
        counter[0] += 1
        return f"{prefix}{counter[0]}"

    def value(expression):
        return eval(expression, dict(SAFE_MATH), dict(env))

    origin = new_id("p")
    entities.append({"id": origin, "type": "point", "at": [0, 0], "fixed": True})

    def placed(x, y):
        uid = new_id("p")
        entities.append({"id": uid, "type": "point", "at": [value(x), value(y)]})
        for kind, expression in (("horizontal_distance", x), ("vertical_distance", y)):
            dimensions_out.append({"id": new_id("k"), "type": kind, "a": origin, "b": uid, "signed": True,
                                   "value": expression})
        return uid

    for loop in loops:
        ids = [placed(item[-2], item[-1]) for item in loop]
        for i, item in enumerate(loop):
            start, end = ids[i - 1], ids[i]
            if item[0] == "arc":
                centre = placed(item[1], item[2])
                entities.append({"id": new_id("c"), "type": "arc", "center": centre, "start": start, "end": end})
            else:
                entities.append({"id": new_id("c"), "type": "line", "start": start, "end": end})
    return {"type": "sketch", "plane": "xy", "entities": entities, "dimensions": dimensions_out}


def profiled_frames(dimensions):
    """A profiled board's frames: its ends, a face frame on each side of
    its box, and for a board that joins the next its `groove` (at the box's
    -X side, z -X) and `tongue` (the cover width on, z +X), at y = 0."""
    hx, hy = BOARD_PROFILES[dimensions["profile"]]["half"]
    frames = [
        {"name": "end_a", "kind": "end"},
        {"name": "end_b", "kind": "end"},
        {"name": "face_xp", "kind": "face", "origin": [hx, 0, 0], "z": [1, 0, 0], "x": [0, 0, 1]},
        {"name": "face_xn", "kind": "face", "origin": [f"-{hx}" if " " not in hx else f"-({hx})", 0, 0],
         "z": [-1, 0, 0], "x": [0, 0, 1]},
        {"name": "face_yp", "kind": "face", "origin": [0, hy, 0], "z": [0, 1, 0], "x": [0, 0, 1]},
        {"name": "face_yn", "kind": "face", "origin": [0, f"-{hy}" if " " not in hy else f"-({hy})", 0],
         "z": [0, -1, 0], "x": [0, 0, 1]},
    ]
    if dimensions["profile"] != "deck":
        cover = BOARD_PROFILES[dimensions["profile"]]["cover"]
        groove = f"-{hx}" if " " not in hx else f"-({hx})"
        frames += [
            {"name": "groove", "kind": "groove", "origin": [groove, 0, 0], "z": [-1, 0, 0], "x": [0, 0, 1]},
            {"name": "tongue", "kind": "tongue", "origin": [f"{cover} - {hx}" if " " not in hx
                                                           else f"{cover} - ({hx})", 0, 0],
             "z": [1, 0, 0], "x": [0, 0, 1]},
        ]
    return frames


# Steel tubes (mitcad#873): round and square hollow sections in the
# standards' sizes, Mitcad's own parts. The section is exact (a square
# tube's outer and inner corners rounded by the standard's radii), so that
# a row's mass per metre is the standard's. It lies as a board's: centred
# on the origin of XY, extruded by the length along +Z. The sizes each
# shape names, as parameters of the design.
TUBE_SHAPES = {
    "round_tube": ("diameter", "thickness", "length"),
    "square_tube": ("size", "thickness", "outer_radius", "inner_radius", "length"),
    # A rectangular tube (mitcad#892): its width along X, its height along Y.
    "rect_tube": ("width", "height", "thickness", "outer_radius", "inner_radius", "length"),
}
# An inner radius below this (mm) is a sharp corner.
SHARP = 0.001


def square_loop(low, high, radius, y_low=None, y_high=None):
    """A square's loop (board_loop's items) from `low` to `high` on both
    axes, or a rectangle's from `y_low` to `y_high` on Y,
    counter-clockwise, its corners rounded by `radius`, or sharp when it
    is None; each an expression."""
    y_low, y_high = y_low or low, y_high or high
    if radius is None:
        return [("point", low, y_low), ("point", high, y_low), ("point", high, y_high), ("point", low, y_high)]
    a, b = f"{low} + {radius}", f"{high} - {radius}"
    c, d = f"{y_low} + {radius}", f"{y_high} - {radius}"
    # From the bottom's left end, reached by the bottom left corner's arc
    # from the left side's lower end (the loop's last point).
    return [("arc", a, c, a, y_low), ("point", b, y_low), ("arc", b, c, high, c), ("point", high, d),
            ("arc", b, d, b, y_high), ("point", a, y_high), ("arc", a, d, low, d), ("point", low, c)]


def circle_sketch(diameter, env):
    """A circle of the expression `diameter` on a fixed point at the origin
    of XY, dimensioned by it."""
    size = eval(diameter, dict(SAFE_MATH), dict(env))
    return {"type": "sketch", "plane": "xy",
            "entities": [{"id": "p1", "type": "point", "at": [0, 0], "fixed": True},
                         {"id": "c2", "type": "circle", "center": "p1", "radius": size / 2}],
            "dimensions": [{"id": "k3", "type": "diameter", "curve": "c2", "value": diameter}]}


def tube_radii(rules, thickness, what):
    """A square tube's corner radii (outer, inner) for a wall `thickness`
    by the rules `[[up_to, outer, inner], …]` (each radius a factor of the
    thickness): the first whose `up_to` is the thickness or more, or
    null."""
    for rule in rules or []:
        if not isinstance(rule, list) or len(rule) != 3:
            fail(f"{what}: sizes.radii is a list of [up_to, outer, inner]")
        if rule[0] is None or thickness <= rule[0] + 1e-9:
            return rule[1] * thickness, rule[2] * thickness
    fail(f"{what}: no rule of sizes.radii takes the wall thickness {thickness:g}")


def tube_area(shape, size, thickness, outer=0.0, inner=0.0, height=None):
    """A tube's section area, mm²: a round tube's annulus; a square tube's
    (a rectangular one's of the `height`) outer square less its inner
    one, each corner rounded by its radius (r² (4 - π) less than the
    square's)."""
    if shape == "round_tube":
        return math.pi / 4 * (size * size - (size - 2 * thickness) ** 2)
    height = size if height is None else height
    hole = (size - 2 * thickness) * (height - 2 * thickness)
    return size * height - (4 - math.pi) * outer * outer - (hole - (4 - math.pi) * inner * inner)


def check_tube(shape, size, thickness, outer, inner, what, height=None):
    """Refuses a tube's sizes that make no section: a wall over half its
    size (a rectangular tube's smaller side), a radius that leaves its
    side no straight part."""
    problems = []
    size = min(size, size if height is None else height)
    if not 0 < thickness < size / 2:
        problems.append(f"the wall {thickness:g} is not between 0 and half the size {size:g}")
    elif shape != "round_tube":
        if not SHARP <= outer < size / 2:
            problems.append(f"the outer radius {outer:g} is not between {SHARP:g} and half the size {size:g}")
        if inner < 0 or 0 < inner < SHARP or inner >= size / 2 - thickness:
            problems.append(f"the inner radius {inner:g} is neither sharp (0) nor between {SHARP:g} and half "
                            f"the bore {size / 2 - thickness:g}")
    if problems:
        fail(f"{what}: {'; '.join(problems)}")


def tube_start(entry, dimensions, parameters):
    """A tube's parameters as its design is made: `parameters` with the
    values of its default row; for a square tube whose rows have sharp and
    rounded inner corners, of its first rounded row (a corner's arc is not
    drawn at a radius of 0), the default row set after the table. And
    whether its rows have sharp and rounded inner corners."""
    shape = dimensions["shape"]
    for key in TUBE_SHAPES[shape]:
        name = dimensions[key]
        if not isinstance(name, str) or not mitcad_name(name):
            fail(f"{entry['id']}: dimensions.{key} of a tube names its parameter")
    rows = entry.get("rows") or []
    out = dict(parameters)
    inner = dimensions.get("inner_radius", "").strip()

    def radius(values):
        return length_of(values.get(inner, 0), f"{entry['id']}: {inner}")

    radii = [radius(row.get("values") or {}) for row in rows] if inner else []
    if inner and not rows:
        radii = [radius(out)]
    sharp = any(r < SHARP for r in radii)
    rounded = any(r >= SHARP for r in radii)
    start = None
    if rows:
        default = entry.get("default") or rows[0]["name"]
        start = next((row for row in rows if row["name"] == default), rows[0])
        if sharp and rounded and radius(start.get("values") or {}) < SHARP:
            start = next(row for row, r in zip(rows, radii) if r >= SHARP)
        out.update(start.get("values") or {})
    missing = [dimensions[key].strip() for key in TUBE_SHAPES[shape] if dimensions[key].strip() not in out]
    if missing:
        fail(f"{entry['id']}: the tube's parameters {', '.join(missing)} have no values")
    return out, sharp, rounded


def mitcad_name(text):
    """Whether a text is a parameter's name (letters, digits and _, not
    first a digit)."""
    return bool(re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", text.strip()))


def tube_checked(mcp, document, entry, dimensions, parameters, sharp, rounded):
    """The design's volume against the section's area times the length:
    in its default row, and when its rows have sharp and rounded inner
    corners, in a row of the other kind too (its bores switched by their
    conditions); the default row again after."""
    shape = dimensions["shape"]
    names = {key: dimensions[key].strip() for key in TUBE_SHAPES[shape]}
    rows = entry.get("rows") or []
    default = (entry.get("default") or rows[0]["name"]) if rows else None

    def numbers(values):
        merged = dict(parameters)
        merged.update(values)
        return {key: length_of(merged.get(name, 0), f"{entry['id']}: {name}") for key, name in names.items()}

    checks = [None]
    if rows:
        checks = [next(row for row in rows if row["name"] == default)]
        if sharp and rounded:
            corner = numbers(checks[0].get("values") or {}).get("inner_radius", 0) >= SHARP
            checks.append(next(row for row in rows
                               if (numbers(row.get("values") or {})["inner_radius"] >= SHARP) != corner))
    for row in checks + ([checks[0]] if len(checks) > 1 else []):
        if row is not None:
            mcp.command(document, {"cmd": "apply_configuration", "name": row["name"]})
        n = numbers((row or {}).get("values") or {})
        area = tube_area(shape, n.get("diameter", n.get("size", n.get("width"))), n["thickness"],
                         n.get("outer_radius", 0.0), n.get("inner_radius", 0.0), n.get("height"))
        volume = (mcp.query(document, {"query": "properties"}).get("total") or {}).get("volume") or 0.0
        expected = area * n["length"]
        if abs(volume - expected) > 1e-6 * expected:
            raise PartError(f"{(row or {}).get('name', 'the tube')}: the section's volume {volume:.3f} mm3 is not "
                            f"its area {area:.3f} mm2 times its length {n['length']:g} mm")


# The `mount` frame of a bracket, hanger or post base is the design's
# origin and axes: z into the member it is placed on (the carrier; the
# post), x up the carried member's section (a hanger), out of the side it
# sits on (a bracket) or along the post's +X.
MOUNT = {"name": "mount", "kind": "face", "origin": [0, 0, 0], "z": [0, 0, 1], "x": [1, 0, 0]}


def timber_connector(part, shape, dimensions):
    """The plates (name, low corner, high corner), the screw holes and the
    frames of a bracket, joist hanger or post base, in its mount frame's
    coordinates. A hole is cut from the surface its fastener's head bears
    on (its frame's origin, z out towards the head) through the plate;
    hole_a_<n> go into member a (the carried member, the post), hole_b_<n>
    into b (the carrier)."""
    d = {key: length_of(value, f"{part}: dimensions.{key}") for key, value in dimensions.items()
         if key not in ("shape", "legs", "holes")}
    hole = d.get("hole", 5.0)
    plates, holes = [], []

    def drill(on, origin, z, depth):
        n = sum(1 for h in holes if h["name"].startswith(f"hole_{on}_")) + 1
        holes.append({"name": f"hole_{on}_{n}", "origin": [round(v, 6) + 0.0 for v in origin], "z": z,
                      "diameter": hole, "depth": depth})

    t = d["thickness"]
    if shape == "bracket":
        # An angle: one arm on the carrier's face (the plane z = 0, from
        # the edge along +x), one on member a's side (the plane x = 0,
        # along -z), `width` across y.
        legs = dimensions["legs"]
        if not isinstance(legs, list) or len(legs) != 2:
            fail(f"{part}: dimensions.legs is a list of two lengths (on the carrier, on the member)")
        on_b, on_a = (length_of(v, f"{part}: dimensions.legs") for v in legs)
        w = d["width"]
        plates = [("Arm on the carrier", [0, -w / 2, -t], [on_b, w / 2, 0]),
                  ("Arm on the member", [0, -w / 2, -on_a], [t, w / 2, 0])]
        spec = dimensions.get("holes") or {}
        for x, y in spec.get("b", []):
            drill("b", [x, y, -t], [0, 0, -1], 2 * t)
        for z, y in spec.get("a", []):
            drill("a", [t, y, -z], [1, 0, 0], 2 * t)
        return plates, holes, [MOUNT]
    if shape == "hanger":
        # A U round the joist's end (`width` x `height`, x up), its flanges
        # on the carrier's face (z = 0); the joist's end at the origin.
        w, h, high, depth, f = d["width"], d["height"], d["hanger_height"], d["depth"], d["flange"]
        bottom, top = -h / 2 - t, -h / 2 + high
        plates = [("Seat", [bottom, -w / 2 - t, -depth], [-h / 2, w / 2 + t, 0]),
                  ("Side 1", [bottom, w / 2, -depth], [top, w / 2 + t, 0]),
                  ("Side 2", [bottom, -w / 2 - t, -depth], [top, -w / 2, 0]),
                  ("Flange 1", [bottom, w / 2, -t], [top, w / 2 + t + f, 0]),
                  ("Flange 2", [bottom, -w / 2 - t - f, -t], [top, -w / 2, 0])]
        count = max(2, int(high // 40))
        for side in (1, -1):
            for i in range(count):
                drill("b", [-h / 2 + high * (i + 0.5) / count, side * (w / 2 + t + f / 2), -t], [0, 0, -1], 2 * t)
        for side in (1, -1):
            for i in (1, 2):
                drill("a", [-h / 2 + high * i / 3, side * (w / 2 + t), -depth / 2], [0, side, 0], 2 * t)
        return plates, holes, [MOUNT]
    # A post base: a plate under the post's end (z < 0) and two side plates
    # up its x+ and x- faces.
    w, high, size, below, side_width = d["width"], d["height"], d["plate"], d["plate_thickness"], d["side"]
    plates = [("Base plate", [-size / 2, -size / 2, -below], [size / 2, size / 2, 0]),
              ("Side 1", [w / 2, -side_width / 2, -below], [w / 2 + t, side_width / 2, high]),
              ("Side 2", [-w / 2 - t, -side_width / 2, -below], [-w / 2, side_width / 2, high])]
    for side in (1, -1):
        drill("a", [side * (w / 2 + t), -side_width / 4, high / 3], [side, 0, 0], 2 * t)
        drill("a", [side * (w / 2 + t), side_width / 4, 2 * high / 3], [side, 0, 0], 2 * t)
    base = {"name": "base", "kind": "face", "origin": [0, 0, -below], "z": [0, 0, -1], "x": [1, 0, 0]}
    return plates, holes, [MOUNT, base]


def fill_fasteners(entry, build):
    """A connector's fasteners given by the member they go into (`on`)
    without frames: every hole frame of that member (hole_<on>_<n>), and
    their count."""
    connector = build.get("connector")
    if not isinstance(connector, dict):
        return
    names = [f["name"] for f in build.get("frames") or []]
    for fastener in connector.get("fasteners") or []:
        if not isinstance(fastener, dict) or "frame" in fastener or "frames" in fastener:
            continue
        on = fastener.get("on")
        if on not in ("a", "b"):
            continue
        frames = [n for n in names if n.startswith(f"hole_{on}_")]
        if not frames:
            raise PartError(f"its fasteners into {on} have no hole frames hole_{on}_<n>")
        fastener["frames"] = frames
        fastener["count"] = len(frames)


# The end grain's tile (mm) and the pith's place in it: the rings of a
# log whose pith lies 40 mm across and 120 mm up from a board's centre, so
# that every section up to 150 mm thick and 300 mm wide shows arcs of one
# pith, the tile's seams (where another tile's pith is as near) outside it.
END_GRAIN_TILE = 400.0
END_GRAIN_PITH = (40.0, 120.0)
# The largest embedded end grain image, base64 (the published library's
# designs keep their textures small).
END_GRAIN_DATA = 64 << 10


def texture_png(kind, colour, seed, pixels=None):
    """A tiling texture image (PNG): `grain` (wood with the grain along the
    image's height), `end_grain` (growth rings, one END_GRAIN_TILE square
    of `pixels`, mitcad#484), `strands` (oriented strand board) or
    `particles` (chipboard), in shades of `colour` (sRGB, 0..1)."""
    rng = random.Random(f"{kind}:{seed}")
    if kind == "grain":
        width, height = 64, 256
    elif kind == "end_grain":
        width = height = pixels or 256
    else:
        width, height = 96, 96
    shades = [[1.0] * width for _ in range(height)]
    if kind == "end_grain":
        # The distance to the nearest of the tiles' piths, so that the
        # image tiles; the rings a little uneven and the latewood dark.
        spacing = rng.uniform(7.5, 9.0)
        p1, p2 = rng.uniform(0, 2 * math.pi), rng.uniform(0, 2 * math.pi)
        size = END_GRAIN_TILE
        for r in range(height):
            for c in range(width):
                dx = (c + 0.5) / width * size - END_GRAIN_PITH[0]
                dy = (r + 0.5) / height * size - END_GRAIN_PITH[1]
                dx -= size * round(dx / size)
                dy -= size * round(dy / size)
                radius = math.hypot(dx, dy)
                angle = math.atan2(dy, dx)
                phase = radius / spacing + 0.18 * math.sin(radius / 23 + p1) + 0.12 * math.sin(3 * angle + p2)
                f = phase - math.floor(phase)
                # Earlywood light, the latewood a soft dark band.
                late = max(0.0, 1.0 - abs(f - 0.8) / 0.22)
                late = late * late * (3 - 2 * late)
                # Few levels: the image stays small enough to embed.
                shades[r][c] = round((1.0 - 0.28 * late) * 24) / 24
    elif kind == "grain":
        streak = [rng.uniform(-1, 1) for _ in range(width)]
        streak = [(streak[i - 1] + 2 * streak[i] + streak[(i + 1) % width]) / 4 for i in range(width)]
        p1, p2, p3 = (rng.uniform(0, 2 * math.pi) for _ in range(3))
        for r in range(height):
            v = r / height
            wobble = 0.04 * math.sin(2 * math.pi * v + p1) + 0.02 * math.sin(6 * math.pi * v + p2)
            for c in range(width):
                u = c / width
                ring = 0.5 + 0.5 * math.sin(10 * math.pi * (u + wobble) + p3 + 0.8 * math.sin(4 * math.pi * u + p1))
                shades[r][c] = 0.95 + 0.05 * streak[c] - 0.25 * ring ** 8
    elif kind == "strands":
        for _ in range(140):
            cx, cy = rng.randrange(width), rng.randrange(height)
            a, b = rng.randint(2, 6), rng.randint(6, 18)
            if rng.random() < 0.5:
                a, b = b, a
            shade = rng.uniform(0.78, 1.15)
            for dy in range(-b, b + 1):
                for dx in range(-a, a + 1):
                    shades[(cy + dy) % height][(cx + dx) % width] = shade
    else:
        for r in range(height):
            for c in range(width):
                shades[r][c] = 0.7 if rng.random() < 0.03 else rng.uniform(0.88, 1.06)
    rows = []
    for r in range(height):
        line = []
        for c in range(width):
            line.extend(max(0, min(255, int(255 * channel * shades[r][c]))) for channel in colour)
        rows.append(line)
    return png(width, height, rows)


def set_appearance(mcp, document, entry, caps=()):
    """The bodies' appearance: `appearance` names one of Mitcad's library
    (`steel_satin`), or is a new one of the design `{id, name, based_on,
    base_color, roughness, texture, size}` whose texture (`grain`,
    `strands`, `particles`) is made here and embedded, projected in the
    body's coordinates so that a board's grain runs along its length (Z).
    A board's `caps` (the faces of its ends) with a grain get its end
    grain, `<id>-end`."""
    # A finish by name (`zinc_plated`, `stainless`, …; library_previews).
    look = previews.finish(entry.get("appearance"))
    if not look:
        return
    if isinstance(look, str):
        appearance = look
    else:
        appearance = look.get("id") or "timber"
        command = {"cmd": "create_appearance", "id": appearance, "name": look.get("name") or entry["name"],
                   "based_on": look.get("based_on", "wood_maple")}
        for key in ("base_color", "roughness", "specular"):
            if key in look:
                command[key] = look[key]
        if look.get("texture"):
            image = texture_png(look["texture"], look.get("base_color", [0.85, 0.72, 0.52]), entry["id"])
            # `path` names the image; its data is embedded in the design.
            command["texture"] = {"path": f"{appearance}-{look['texture']}.png",
                                  "data": base64.b64encode(image).decode("ascii"),
                                  "size": look.get("size", [100, 1000]), "projection": "box"}
        mcp.command(document, command)
    bodies = mcp.query(document, {"query": "bodies"})
    for body in bodies:
        mcp.command(document, {"cmd": "set_body_appearance", "uid": body["uid"], "appearance": appearance})
    # A board's ends (mitcad#484): growth rings of a second appearance on
    # the caps of its extrusion, which placed members show on their ends.
    if caps and isinstance(look, dict) and look.get("texture") == "grain":
        colour = [round(0.9 * c, 3) for c in look.get("base_color", [0.85, 0.72, 0.52])]
        for pixels in (256, 192, 128):
            data = base64.b64encode(texture_png("end_grain", colour, entry["id"], pixels)).decode("ascii")
            if len(data) <= END_GRAIN_DATA:
                break
        else:
            raise PartError(f"the end grain's image is larger than {END_GRAIN_DATA >> 10} KiB")
        ends = f"{appearance}-end"
        mcp.command(document, {"cmd": "create_appearance", "id": ends,
                               "name": f"{look.get('name') or entry['name']}, end grain",
                               "based_on": look.get("based_on", "wood_maple"), "base_color": colour,
                               "roughness": min(1.0, look.get("roughness", 0.7) + 0.1),
                               "texture": {"path": f"{ends}-end_grain.png", "data": data,
                                           "size": [END_GRAIN_TILE, END_GRAIN_TILE], "projection": "box"}})
        for body in bodies:
            mcp.command(document, {"cmd": "set_face_appearance", "uid": body["uid"], "faces": caps,
                                   "appearance": ends})


def make_model(ctx, mcp, entry, build):
    """A sheet, plate, angle or rod from its `dimensions`: `shape`
    (`plate`, `angle`, `rod` or `none`), its sizes (a parameter of
    `parameters`, or a value, which becomes a user parameter) and
    `parameters` (user parameters and their defaults); then any
    `commands`. A plate and an angle are centred on x = 0 across their
    `width`: a plate runs `thickness` along +Y and `length` along +Z; an
    angle has its first leg along +Z (in the XZ plane, `thickness` along
    +Y) and its second along +Y (in the XY plane, `thickness` along +Z). A
    rod is a cylinder of `diameter` on the Z axis from z = 0 to `length`;
    with `thread` (a thread size: `standard`, `designation`, `class`) a
    cosmetic thread over its whole side (mitcad#449).

    The timber parts (mitcad#478): a `board` is a rectangle `width` x
    `thickness` centred on XY (width along X) extruded by `length` along
    +Z, the layout of a profile, with its ends and four face frames that
    follow the width and thickness (with a `profile`, a profiled board's
    section: BOARD_PROFILES, profiled_sketch); a `fastener` a wood screw or nail (a
    shank of `diameter` from z = 0 to `length`, a head of `head_diameter`
    and `head_height`, countersunk into z > 0 or bearing on z = 0); a
    `sheet` a box from the origin, `width` along X, `height` along Y,
    `thickness` along +Z; a `bracket`, `hanger` and `post_base` steel
    plates of fixed sizes built in their `mount` frame's coordinates, with
    their screw holes cut and named as hole frames (timber_connector).

    The steel tubes (mitcad#873): a `round_tube` (`diameter`,
    `thickness`, `length`) and a `square_tube` (`size`, `thickness`,
    `outer_radius`, `inner_radius`, `length`), and (mitcad#892) a
    `rect_tube` (`width` along X, `height` along Y, and the square's
    others), each size a parameter's name, centred on XY: the outside
    extruded by the length along +Z, the bore cut along all of it
    (TUBE_SHAPES, tube_start); a square or rectangular tube's ends and
    four face frames, a round tube's ends and `axis`."""
    dimensions = entry.get("dimensions") or {}
    if not isinstance(dimensions, dict):
        fail(f"{entry['id']}: dimensions is no object")
    shape = dimensions.get("shape", "none")
    needed = {"plate": ("width", "thickness", "length"), "angle": ("legs", "width", "thickness"),
              "rod": ("diameter", "length"), "none": (),
              "board": ("width", "thickness", "length"),
              "fastener": ("diameter", "length", "head_diameter", "head_height"),
              "sheet": ("width", "height", "thickness"),
              "bracket": ("legs", "width", "thickness"),
              "hanger": ("width", "height", "hanger_height", "depth", "flange", "thickness"),
              "post_base": ("width", "height", "plate", "plate_thickness", "side", "thickness"),
              **TUBE_SHAPES}.get(shape)
    if needed is None:
        fail(f"{entry['id']}: dimensions.shape '{shape}' is none of plate, angle, rod, none, board, fastener, "
             "sheet, bracket, hanger, post_base, round_tube, square_tube, rect_tube")
    missing = [n for n in needed if n not in dimensions]
    if missing:
        fail(f"{entry['id']}: the {shape} needs dimensions.{', dimensions.'.join(missing)}")
    profile = dimensions.get("profile")
    if profile is not None and (shape != "board" or profile not in BOARD_PROFILES):
        fail(f"{entry['id']}: dimensions.profile '{profile}' is a board's, one of {', '.join(BOARD_PROFILES)}")
    if dimensions.get("v_joint") and profile != "tongue_and_groove" or (
            dimensions.get("grooves") and profile != "deck"):
        fail(f"{entry['id']}: a V-joint is a tongue and groove board's, grooves a deck board's")
    parameters = dimensions.get("parameters") or {}
    if not isinstance(parameters, dict):
        fail(f"{entry['id']}: dimensions.parameters is no object")
    thread = dimensions.get("thread")
    if thread is not None and (shape != "rod" or not isinstance(thread, dict) or not thread.get("designation")):
        fail(f"{entry['id']}: dimensions.thread is a rod's thread size with a designation")
    sharp = rounded = False
    if shape in TUBE_SHAPES:
        # Its parameters take a row's values (tube_start).
        parameters, sharp, rounded = tube_start(entry, dimensions, parameters)
    document = mcp.create()
    dress = None
    try:
        names = set()
        for name, value in parameters.items():
            # A length, or an angle in degrees (a profiled board's bevel).
            angle = angle_value(value)
            mcp.command(document, {"cmd": "add_parameter", "name": name, "expression": angle[0] if angle
                                   else mm(length_of(value, f"dimensions.parameters.{name}"))})
            names.add(name)

        def size(value, default_name):
            """The parameter a size names, or a new user parameter of its value."""
            if isinstance(value, str) and value.strip() in names:
                return value.strip()
            millimetres = length_of(value, f"{entry['id']}: dimensions")
            name, n = default_name, 1
            while name in names:
                n += 1
                name = f"{default_name}{n}"
            mcp.command(document, {"cmd": "add_parameter", "name": name, "expression": mm(millimetres)})
            names.add(name)
            return name

        def box(name, length, width, height, operation):
            return mcp.add(document, {"cmd": "add_feature", "name": name, "def": {
                "type": "box", "plane": "xy", "corner": [0, 0], "length": length, "width": width,
                "height": height, "operation": operation}})

        # The frames a shape makes when the entry lists none, and the holes
        # it has cut (their frames are added to the others).
        default_frames, holes, caps = [], [], []
        if shape == "plate":
            w = size(dimensions["width"], "W")
            t = size(dimensions["thickness"], "T")
            length = size(dimensions["length"], "L")
            box("Plate", w, t, length, "new_body")
        elif shape == "angle":
            legs = dimensions["legs"]
            if not isinstance(legs, list) or len(legs) != 2:
                fail(f"{entry['id']}: dimensions.legs is a list of two lengths")
            a, b = size(legs[0], "A"), size(legs[1], "B")
            w = size(dimensions["width"], "W")
            t = size(dimensions["thickness"], "T")
            box("Leg A", w, t, a, "new_body")
            box("Leg B", w, b, t, "join")
        elif shape == "rod":
            d = size(dimensions["diameter"], "D")
            length = size(dimensions["length"], "L")
            rod = mcp.add(document, {"cmd": "add_feature", "name": "Rod", "def": {
                "type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": d, "height": length,
                "operation": "new_body"}})
            if thread is not None:
                # Cosmetic, over the whole side: the thread follows the
                # length, and the threads query lists it.
                body = mcp.query(document, {"query": "bodies"})[0]["uid"]
                size_of = {key: thread[key] for key in ("standard", "designation", "class", "right_handed")
                           if key in thread}
                mcp.add(document, {"cmd": "add_feature", "name": "Thread", "def": {
                    "type": "thread", "faces": [{"body": body, "face": f"{rod}:side0"}], "thread": size_of,
                    "modeled": False}})
                threads = mcp.query(document, {"query": "threads"}) or []
                if len(threads) != 1:
                    raise PartError(f"the rod's thread {thread['designation']} is not listed "
                                    f"({len(threads)} threads)")
        elif shape == "board" and dimensions.get("profile"):
            # A profiled board (mitcad#477): its section's points follow
            # the board's parameters, its profile's derived ones after them.
            w = size(dimensions["width"], "W")
            t = size(dimensions["thickness"], "T")
            length = size(dimensions["length"], "L")
            if (w, t) != ("W", "T"):
                fail(f"{entry['id']}: a profiled board's width and thickness are the parameters W and T")
            values = board_parameters(dimensions)
            for name, value in values.items():
                if name not in names:
                    angle = angle_value(value)
                    mcp.command(document, {"cmd": "add_parameter", "name": name,
                                           "expression": angle[0] if angle else mm(length_of(value, name))})
                    names.add(name)
            for name, expression in BOARD_PROFILES[dimensions["profile"]].get("derived", []):
                mcp.command(document, {"cmd": "add_parameter", "name": name, "expression": expression})
                names.add(name)
            env = board_numbers(dimensions, values)
            area, _, _ = board_shape(dimensions, env)
            if area <= 0:
                raise PartError(f"the {dimensions['profile']} section has no area at its default sizes")
            sketch = mcp.add(document, {"cmd": "add_feature", "name": "Section",
                                        "def": profiled_sketch(dimensions, env)})
            regions = mcp.query(document, {"query": "sketch", "uid": sketch}).get("regions") or []
            if len(regions) != 1:
                raise PartError(f"the board's section has {len(regions)} regions, not one")
            region = regions[0]["key"]
            extrude = mcp.add(document, {"cmd": "add_feature", "name": "Profile", "def": {
                "type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                "extent": {"type": "distance", "distance": length}, "operation": "new_body"}})
            caps = [f"{extrude}:start({region})", f"{extrude}:end({region})"]
            default_frames = profiled_frames(dimensions)
            # The design's volume is the area the rows' blocks give times
            # the length.
            volume = (mcp.query(document, {"query": "properties"}).get("total") or {}).get("volume") or 0.0
            expected = area * env["L"]
            if abs(volume - expected) > 1e-6 * expected:
                raise PartError(f"the section's volume {volume:.3f} mm3 is not its area {area:.3f} mm2 "
                                f"times its length {env['L']:g} mm")
            build.setdefault("board", board_block(dimensions, env))
        elif shape == "board":
            w = size(dimensions["width"], "W")
            t = size(dimensions["thickness"], "T")
            length = size(dimensions["length"], "L")
            sketch = mcp.add(document, {"cmd": "add_feature", "name": "Section", "def": board_sketch(
                w, t, value_of(dimensions["width"], parameters), value_of(dimensions["thickness"], parameters))})
            regions = mcp.query(document, {"query": "sketch", "uid": sketch}).get("regions") or []
            if len(regions) != 1:
                raise PartError(f"the board's section has {len(regions)} regions, not one")
            region = regions[0]["key"]
            extrude = mcp.add(document, {"cmd": "add_feature", "name": "Profile", "def": {
                "type": "extrude", "profiles": [{"sketch": sketch, "region": region}],
                "extent": {"type": "distance", "distance": length}, "operation": "new_body"}})
            caps = [f"{extrude}:start({region})", f"{extrude}:end({region})"]
            default_frames = board_frames(w, t)
        elif shape in TUBE_SHAPES:
            # A steel tube (mitcad#873): its outside extruded by the
            # length, its bore cut along all of it. A square tube whose
            # rows have sharp and rounded inner corners has both bores,
            # each suppressed by a condition of the inner radius.
            p = {key: dimensions[key].strip() for key in TUBE_SHAPES[shape]}
            env = {name: length_of(value, f"{entry['id']}: {name}") for name, value in parameters.items()
                   if not angle_value(value)}
            length = p["length"]
            if shape == "round_tube":
                d, t = p["diameter"], p["thickness"]
                outside = circle_sketch(d, env)
                bores = [("Bore", circle_sketch(f"{d} - 2 * {t}", env), None)]
                default_frames = [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                                  {"name": "axis", "kind": "shaft", "origin": [0, 0, 0], "z": [0, 0, 1],
                                   "x": [1, 0, 0]}]
            else:
                # A square tube's sides, or a rectangular one's width and
                # height.
                b, t, ro, ri = p.get("size", p.get("width")), p["thickness"], p["outer_radius"], p["inner_radius"]
                h = p.get("height", b)
                outside = points_sketch([square_loop(f"-{b} / 2", f"{b} / 2", ro, f"-{h} / 2", f"{h} / 2")], env)
                inside = (f"{t} - {b} / 2", f"{b} / 2 - {t}", f"{t} - {h} / 2", f"{h} / 2 - {t}")
                bores = []
                if rounded or not sharp:
                    bores.append(("Bore", points_sketch([square_loop(inside[0], inside[1], ri, *inside[2:])], env),
                                  f"{ri} < {SHARP:g} mm" if sharp else None))
                if sharp:
                    bores.append(("Sharp bore", points_sketch([square_loop(inside[0], inside[1], None, *inside[2:])],
                                                              env),
                                  f"{ri} >= {SHARP:g} mm" if rounded else None))
                default_frames = board_frames(b, h)

            def one_region(sketch, what):
                regions = mcp.query(document, {"query": "sketch", "uid": sketch}).get("regions") or []
                if len(regions) != 1:
                    raise PartError(f"the tube's {what} has {len(regions)} regions, not one")
                return regions[0]["key"]

            sketch = mcp.add(document, {"cmd": "add_feature", "name": "Section", "def": outside})
            mcp.add(document, {"cmd": "add_feature", "name": "Tube", "def": {
                "type": "extrude", "profiles": [{"sketch": sketch, "region": one_region(sketch, "outside")}],
                "extent": {"type": "distance", "distance": length}, "operation": "new_body"}})
            for name, definition, suppressed_when in bores:
                sketch = mcp.add(document, {"cmd": "add_feature", "name": f"{name} section", "def": definition})
                cut = mcp.add(document, {"cmd": "add_feature", "name": name, "def": {
                    "type": "extrude", "profiles": [{"sketch": sketch, "region": one_region(sketch, name.lower())}],
                    "extent": {"type": "distance", "distance": length}, "operation": "cut"}})
                if suppressed_when:
                    # The cut first: its sketch is suppressed after it.
                    for uid in (cut, sketch):
                        mcp.command(document, {"cmd": "suppress_feature", "uid": uid, "expression": suppressed_when})
        elif shape == "fastener":
            d = size(dimensions["diameter"], "D")
            length = size(dimensions["length"], "L")
            dk = size(dimensions["head_diameter"], "DK")
            k = size(dimensions["head_height"], "K")
            head = dimensions.get("head", "countersunk")
            if head not in ("countersunk", "flat", "pan", "washer"):
                fail(f"{entry['id']}: dimensions.head '{head}' is none of countersunk, flat, pan, washer")
            shank = mcp.add(document, {"cmd": "add_feature", "name": "Shank", "def": {
                "type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": d, "height": length,
                "operation": "new_body"}})
            # A countersunk head lies in the material below the surface (z
            # > 0), the others on it (z < 0).
            head_uid = mcp.add(document, {"cmd": "add_feature", "name": "Head", "def": {
                "type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": dk,
                "height": k if head == "countersunk" else f"-{k}", "operation": "join"}})
            if ((entry.get("build") or {}).get("wood") or {}).get("kind") == "screw":
                # After the frames, whose uids stay those of a plain part.
                dress = (shank, head_uid, head, dk, length, d, value_of(dimensions["diameter"], parameters))
            default_frames = [{"name": "axis", "kind": "shaft", "origin": [0, 0, 0], "z": [0, 0, 1], "x": [1, 0, 0]}]
        elif shape == "sheet":
            w = size(dimensions["width"], "W")
            h = size(dimensions["height"], "H")
            t = size(dimensions["thickness"], "T")
            box("Sheet", w, h, t, "new_body")
            default_frames = [{"name": "face", "kind": "face", "origin": [0, 0, 0], "z": [0, 0, 1], "x": [1, 0, 0]}]
        elif shape in ("bracket", "hanger", "post_base"):
            plates, holes, default_frames = timber_connector(entry["id"], shape, dimensions)
            for n, (name, low, high) in enumerate(plates):
                mcp.add(document, {"cmd": "add_feature", "name": name, "def": {
                    "type": "box", "plane": {"origin": [0, 0, low[2]], "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
                    "corner": [low[0], low[1]], "length": high[0] - low[0], "width": high[1] - low[1],
                    "height": high[2] - low[2], "operation": "new_body" if n == 0 else "join"}})
        if shape in ("plate", "angle"):
            bodies = mcp.query(document, {"query": "bodies"})
            mcp.add(document, {"cmd": "add_feature", "name": "Centred", "def": {
                "type": "move", "bodies": [body["uid"] for body in bodies],
                "transform": {"type": "translate_xyz", "x": f"-{w} / 2", "y": 0, "z": 0}}})
        for n, hole in enumerate(holes, 1):
            # Cut from the surface the head bears on, into the plate.
            z = hole["z"]
            mcp.add(document, {"cmd": "add_feature", "name": f"Hole {n}", "def": {
                "type": "cylinder", "plane": {"origin": hole["origin"], "normal": z,
                                              "x_axis": [0, 0, 1] if abs(z[2]) < 0.9 else [1, 0, 0]},
                "center": [0, 0], "diameter": hole["diameter"], "height": -hole["depth"], "operation": "cut"}})
        for command in entry.get("commands") or []:
            mcp.command(document, command)
        if not mcp.query(document, {"query": "bodies"}):
            raise PartError("the model makes no body")
        listed = (entry.get("frames") or default_frames) + [
            {"name": h["name"], "kind": "hole", "origin": h["origin"], "z": h["z"],
             "x": [0, 0, 1] if abs(h["z"][2]) < 0.9 else [1, 0, 0], "diameter": mm(h["diameter"])} for h in holes]
        frames = listed_frames(listed, entry["id"])
        length = (build.get("length") or {}).get("parameter")
        add_frames(mcp, document, frames, length)
        set_frames(entry, build, frames, build.get("frames") or [])
        fill_fasteners(entry, build)
        if dress:
            wood_screw_dress(mcp, document, entry, *dress)
        set_appearance(mcp, document, entry, caps)
        # A tube's design was made at a row's values: the default row's set
        # after the table, and its volume checked.
        finish_design(mcp, document, entry, apply_default=shape in TUBE_SHAPES)
        if shape in TUBE_SHAPES:
            tube_checked(mcp, document, entry, dimensions, parameters, sharp, rounded)
        return save(ctx, mcp, document, entry), {"kind": "model"}
    finally:
        mcp.close(document)


def finish_design(mcp, document, entry, apply_default=False):
    """The configuration table of the rows (with `apply_default`, its
    default row applied), and a recompute's check."""
    rows = entry.get("rows") or []
    if rows:
        selectors, parameters = [], []
        table_rows = []
        for i, row in enumerate(rows):
            if not isinstance(row, dict) or not row.get("name"):
                fail(f"{entry['id']}: rows[{i}] has no name")
            for key in (row.get("select") or {}):
                if key not in selectors:
                    selectors.append(key)
            for key in (row.get("values") or {}):
                if key not in parameters:
                    parameters.append(key)
            table_row = {"name": row["name"], "values": row.get("values") or {}}
            for key in ("select", "designation", "part_number", "supplier_url"):
                if row.get(key):
                    table_row[key] = row[key]
            table_rows.append(table_row)
        table = {"selectors": selectors, "parameters": parameters,
                 "default": entry.get("default", rows[0]["name"]), "rows": table_rows}
        mcp.command(document, {"cmd": "set_configurations", "configurations": table})
        if apply_default:
            mcp.command(document, {"cmd": "apply_configuration", "name": table["default"]})
    timeline = mcp.query(document, {"query": "timeline"})
    failed = [f"{f.get('name')}: {f.get('error')}" for f in timeline.get("features", []) if f.get("status") == "error"]
    if failed:
        raise PartError("; ".join(failed))


# Top-level fields of a design that put its parts in components: a
# library's design holds its features in the root.
ASSEMBLY_FIELDS = ("components", "occurrences", "active_component", "root_policy")


def save(ctx, mcp, document, entry):
    name = f"part-{entry['id']}.mitcad"
    mcp.save(document, name)
    path = os.path.join(ctx.scratch, name)
    try:
        with open(path, encoding="utf-8") as f:
            design = json.load(f)
    except (OSError, ValueError) as e:
        raise PartError(f"the saved design cannot be read: {e}")
    found = [key for key in ASSEMBLY_FIELDS if design.get(key) not in (None, [], {}, "C0")]
    if found:
        raise PartError(f"the design has {', '.join(found)}: its features are not all in the root")
    # Names a source file brought along in another language or a broken
    # encoding (German or mis-decoded body names) become the part's own: names
    # are for people, references go by uid.
    renamed = {}

    def plain(value):
        if isinstance(value, dict):
            for key, item in value.items():
                if key == "name" and isinstance(item, str) and not item.isascii():
                    value[key] = renamed.setdefault(item, f"{entry['name']} {len(renamed) + 1}")
                else:
                    plain(item)
        elif isinstance(value, list):
            for item in value:
                plain(item)

    plain(design)
    # Other text from the source (its file name in a base feature) is kept,
    # written as JSON escapes so that the design file is ASCII.
    with open(path, encoding="utf-8") as f:
        is_ascii = f.read().isascii()
    if renamed or not is_ascii:
        with open(path, "w", encoding="utf-8") as f:
            json.dump(design, f, indent=2, ensure_ascii=True)
            f.write("\n")
    return path


# ---------------------------------------------------------------------
# Previews: 256 x 256 PNG images.

SIZE = 256


def png(width, height, pixels):
    raw = b"".join(b"\x00" + bytes(row) for row in pixels)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def raster(polygons):
    """Fills polygons (lists of points in a -1..1 square, y up) with their
    shades by the even-odd rule within each polygon group; later groups
    are drawn over earlier ones. Returns the image's rows."""
    background = (246, 247, 249)
    index = [[-1] * SIZE for _ in range(SIZE)]
    shades = [[None] * SIZE for _ in range(SIZE)]
    for number, (segments, shade) in enumerate(polygons):
        for row in range(SIZE):
            y = 1 - 2 * (row + 0.5) / SIZE
            crossings = []
            for (x1, y1), (x2, y2) in segments:
                if (y1 > y) != (y2 > y):
                    crossings.append(x1 + (y - y1) * (x2 - x1) / (y2 - y1))
            crossings.sort()
            for a, b in zip(crossings[0::2], crossings[1::2]):
                first = max(0, int(math.ceil((a + 1) * SIZE / 2 - 0.5)))
                last = min(SIZE - 1, int(math.floor((b + 1) * SIZE / 2 - 0.5)))
                for column in range(first, last + 1):
                    index[row][column] = number
                    shades[row][column] = shade
    pixels = []
    for row in range(SIZE):
        line = []
        for column in range(SIZE):
            shade = shades[row][column]
            if shade is None:
                line.extend(background)
            else:
                # Anodised aluminium, lit from the upper left.
                light = 0.75 + 0.25 * (1 - (row + column) / (2 * SIZE))
                level = max(0.0, min(1.0, shade * light))
                line.extend((int(60 + 170 * level), int(66 + 172 * level), int(76 + 174 * level)))
        pixels.append(line)
    for row in range(1, SIZE - 1):
        for column in range(1, SIZE - 1):
            here = index[row][column]
            if here < 0:
                continue
            around = (index[row - 1][column], index[row + 1][column], index[row][column - 1], index[row][column + 1])
            if any(other != here for other in around):
                pixels[row][3 * column:3 * column + 3] = [38, 42, 50]
    return pixels


def fit(points_lists):
    xs = [x for points in points_lists for x, _ in points]
    ys = [y for points in points_lists for _, y in points]
    span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
    cx, cy = (max(xs) + min(xs)) / 2, (max(ys) + min(ys)) / 2
    scale = 1.7 / span
    return lambda p: ((p[0] - cx) * scale, (p[1] - cy) * scale)


def section_preview(curves):
    """The section seen from end B."""
    chains = [polyline(c, 96) for c in curves]
    to = fit(chains)
    segments = []
    for chain in chains:
        points = [to(p) for p in chain]
        segments.extend(zip(points, points[1:]))
    return png(SIZE, SIZE, raster([(segments, 0.85)]))


def box_preview(low, high):
    """The bounds of the bodies as a box seen from the front right, above."""
    def iso(p):
        x, y, z = p
        return ((x - y) * math.cos(math.radians(30)), (x + y) * math.sin(math.radians(30)) + z)

    x0, y0, z0 = low
    x1, y1, z1 = high
    top = [(x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    front = [(x0, y0, z0), (x1, y0, z0), (x1, y0, z1), (x0, y0, z1)]
    side = [(x1, y0, z0), (x1, y1, z0), (x1, y1, z1), (x1, y0, z1)]
    faces = [[iso(p) for p in face] for face in (front, side, top)]
    to = fit(faces)
    polygons = []
    for face, shade in zip(faces, (0.65, 0.5, 0.9)):
        points = [to(p) for p in face]
        polygons.append((list(zip(points, points[1:] + points[:1])), shade))
    return png(SIZE, SIZE, raster(polygons))


def preview(ctx, design, made, mcp_bounds):
    if made["kind"] == "section":
        return section_preview(made["curves"])
    if ctx.render:
        target = design + ".png"
        result = run([ctx.cli, "render", design, "-o", target, "--view", "iso", "--size", f"{SIZE}x{SIZE}",
                      "--samples", "16", "--time-limit", "30"] + previews.worker_arguments(ctx.worker),
                     check=False)
        if result.returncode == 0 and os.path.isfile(target) and os.path.getsize(target) <= 512 << 10:
            with open(target, "rb") as f:
                return f.read()
    return box_preview(*mcp_bounds)


def end_view(w, t, length):
    """`add_named_view`'s fields for a board w x t (X, Y) from z = 0 to
    `length`, seen from beyond end B and a little from +X and +Y, so that
    the end, the +X side and the top show; orthographic, the piece filling
    the view."""
    look = [-0.3, -0.3, -1.0]  # from the eye to the piece
    norm = math.sqrt(sum(c * c for c in look))
    look = [c / norm for c in look]
    up_hint = [0.0, 1.0, 0.0]
    right = cross(look, up_hint)
    norm = math.sqrt(sum(c * c for c in right))
    right = [c / norm for c in right]
    up = cross(right, look)
    corners = [(x, y, z) for x in (-w / 2, w / 2) for y in (-t / 2, t / 2) for z in (0.0, length)]
    target = [0.0, 0.0, length / 2]
    across = max(abs(dot([c[i] - target[i] for i in range(3)], axis)) for c in corners for axis in (right, up))
    distance = 4 * math.sqrt(w * w + t * t + length * length)
    eye = [target[i] - look[i] * distance for i in range(3)]
    return {"eye": eye, "target": target, "up": up, "perspective": False, "height": 2 * across * 1.1}


def row_previews(ctx, mcp, design, entry):
    """Each row of a timber board (mitcad#484) rendered from its end: a
    piece 1.2 times its larger side long, so that the tiles show the
    sections' proportions; PNG by row name, none without rendering."""
    rows = entry.get("rows") or []
    if not ctx.render or (entry.get("dimensions") or {}).get("shape") != "board" or not rows:
        return {}
    images = {}
    for row in rows:
        values = row.get("values") or {}
        w = length_of(values.get("W"), f"{entry['id']} {row['name']}: W")
        t = length_of(values.get("T"), f"{entry['id']} {row['name']}: T")
        if row.get("_box"):
            # A profiled board's box (a feather-edge board lies turned).
            w, t = row["_box"]
        length = round(1.2 * max(w, t))
        name = f"row-{entry['id']}-{row['name']}.mitcad"
        document = mcp.open(os.path.relpath(design, ctx.scratch).replace("\\", "/"))
        try:
            mcp.command(document, {"cmd": "apply_configuration", "name": row["name"]})
            mcp.command(document, {"cmd": "set_parameter", "name": "L", "expression": f"{length} mm"})
            view = end_view(w, t, length)
            view.update({"cmd": "add_named_view", "name": "End", "replace": True})
            mcp.command(document, view)
            mcp.save(document, name)
        finally:
            mcp.close(document)
        target = os.path.join(ctx.scratch, name + ".png")
        result = run([ctx.cli, "render", os.path.join(ctx.scratch, name), "-o", target, "--view", "End",
                      "--size", f"{SIZE}x{SIZE}", "--samples", "16", "--time-limit", "30"]
                     + previews.worker_arguments(ctx.worker), check=False)
        if result.returncode != 0 or not os.path.isfile(target) or os.path.getsize(target) > 512 << 10:
            raise PartError(f"row {row['name']}'s preview was not rendered: "
                            f"{(result.stdout + result.stderr).strip()[-300:]}")
        with open(target, "rb") as f:
            images[row["name"]] = f.read()
    return images


def design_bounds(mcp, ctx, design):
    document = mcp.open(os.path.relpath(design, ctx.scratch).replace("\\", "/"))
    try:
        bodies = mcp.query(document, {"query": "bodies", "properties": True})
    finally:
        mcp.close(document)
    lows = [b["bbox"]["min"] for b in bodies if "bbox" in b]
    highs = [b["bbox"]["max"] for b in bodies if "bbox" in b]
    if not lows:
        return [0, 0, 0], [1, 1, 1]
    return [min(p[i] for p in lows) for i in range(3)], [max(p[i] for p in highs) for i in range(3)]


# ---------------------------------------------------------------------
# Group files and the library folder.

def tube_rows(entry, dimensions, sizes, spec, what):
    """A steel tube's rows of one outside size (mitcad#873): one for each
    wall thickness of `thicknesses`, named `<D>x<T>` (round),
    `<B>x<B>x<T>` (square) or `<W>x<H>x<T>` (rectangular, mitcad#892), its
    options Diameter or Size (a rectangular tube's `<W>x<H>`) and
    Thickness.
    Its values are the sizes' parameters, a square tube's corner radii by
    `sizes.radii`; its block the section, the mass per metre (the exact
    area times the density), its `tube` sizes and its tolerance."""
    shape = dimensions["shape"]
    names = {key: dimensions.get(key) for key in TUBE_SHAPES[shape]}
    if not all(isinstance(n, str) and mitcad_name(n) for n in names.values()):
        fail(f"{what}: a tube's sizes in dimensions name its parameters")
    density = density_of(entry)
    if density is None:
        fail(f"{what}: a tube's mass per metre needs a density (the part's or its group file's)")
    round_tube = shape == "round_tube"
    rect = shape == "rect_tube"
    key = "diameter" if round_tube else "width" if rect else "size"
    keys = (key, "height") if rect else (key,)
    if not all(isinstance(spec.get(k), (int, float)) for k in keys) or not isinstance(spec.get("thicknesses"), list):
        fail(f"{what}: a {shape}'s row of sizes has {' and '.join(keys)} and thicknesses")
    size = float(spec[key])
    height = float(spec["height"]) if rect else size
    text = lambda v: f"{v:g}"

    def tolerance(thickness):
        """The wall's tolerance: the first rule [up_to, plus_minus, factor]
        whose up_to is the thickness or more, or null; plus or minus the
        smaller of plus_minus and factor times the thickness."""
        for rule in (sizes.get("tolerance") or {}).get("thickness") or []:
            if rule[0] is None or thickness <= rule[0] + 1e-9:
                value = rule[1] if len(rule) < 3 else min(rule[1], rule[2] * thickness)
                return [-round(value, 4), round(value, 4)]
        return None

    rows = []
    for thickness in spec["thicknesses"]:
        thickness = float(thickness)
        outer, inner = (0.0, 0.0) if round_tube else tube_radii(sizes.get("radii"), thickness, what)
        outer, inner = round(outer, 4), round(inner, 4)
        name = f"{text(size)}x{text(thickness)}" if round_tube else f"{text(size)}x{text(height)}x{text(thickness)}"
        check_tube(shape, size, thickness, outer, inner, f"{what} {name}", height)
        area = tube_area(shape, size, thickness, outer, inner, height)
        values = {names[key]: mm(size), names["thickness"]: mm(thickness)}
        if rect:
            values[names["height"]] = mm(height)
        tube = {"shape": "round" if round_tube else "rectangular" if rect else "square", "thickness": mm(thickness)}
        if not round_tube:
            values.update({names["outer_radius"]: mm(outer), names["inner_radius"]: mm(inner)})
            tube.update({"outer_radius": mm(outer), "inner_radius": mm(inner)})
        tube["area"] = round(area, 3)
        build = {"section": {"width": mm(size), "height": mm(height)},
                 "mass_per_metre": f"{area * density * 1e-6:.3f} kg", "tube": tube}
        limits = {}
        if spec.get("tolerance") is not None:
            for k in keys:
                limits[k] = [-float(spec["tolerance"]), float(spec["tolerance"])]
        if tolerance(thickness):
            limits["thickness"] = tolerance(thickness)
        if limits:
            build["tolerance"] = limits
        rows.append({"name": name,
                     "select": {"Diameter" if round_tube else "Size": f"{text(size)}x{text(height)}" if rect else text(size),
                                "Thickness": text(thickness)},
                     "values": values, "build": build})
    return rows


def expand_sizes(entry, group):
    """A part's `sizes` made into its configuration `rows`, each with the
    build block of its own (`build`, the manifest's `build.rows`): a
    `board`'s sizes of timber (thickness x widths, nominal and actual,
    species, density, tolerance, availability), a `fastener`'s diameters x
    lengths, a `sheet`'s thicknesses, a tube's outside sizes x wall
    thicknesses (tube_rows)."""
    what = f"{group}: {entry['id']}"
    sizes = entry.pop("sizes")
    if entry.get("rows"):
        fail(f"{what}: a part has `sizes` or `rows`, not both")
    if not isinstance(sizes, dict) or not isinstance(sizes.get("rows"), list):
        fail(f"{what}: sizes is an object with rows")
    dimensions = entry.get("dimensions") or {}
    shape = dimensions.get("shape")
    profile = dimensions.get("profile") if shape == "board" else None
    if profile is not None and profile not in BOARD_PROFILES:
        fail(f"{what}: dimensions.profile '{profile}' is none of {', '.join(BOARD_PROFILES)}")
    text = lambda v: f"{v:g}"
    rows = []
    for spec in sizes["rows"]:
        if not isinstance(spec, dict):
            fail(f"{what}: a row of sizes is no object")
        availability = spec.get("availability", "standard")
        if shape == "board":
            species = spec.get("species", sizes.get("species_default"))
            densities = sizes.get("species") or {}
            if species not in densities:
                fail(f"{what}: the species {species!r} has no density in sizes.species")
            thickness = float(spec["thickness"])
            covers = spec.get("covers")
            if covers is not None and (not profile or len(covers) != len(spec.get("widths") or [])):
                fail(f"{what}: covers are a profiled board's, one per width")
            for i, width in enumerate(spec.get("widths") or []):
                width = float(width)
                actual = width - float(sizes.get("resawn", 0)) if spec.get("resawn") else width

                def tolerance(size, dimension):
                    for rule in (sizes.get("tolerance") or {}).get(dimension) or []:
                        if rule[0] is None or size <= rule[0]:
                            return [rule[1], rule[2]]
                    return None

                build = {"section": {"width": mm(actual), "height": mm(thickness)},
                         "nominal": {"width": mm(width), "height": mm(thickness)},
                         "availability": availability,
                         "material": {"species": species, "density": densities[species]}}
                if sizes.get("tolerance"):
                    build["tolerance"] = {"thickness": tolerance(thickness, "thickness"),
                                          "width": tolerance(width, "width")}
                values = {"W": mm(actual), "T": mm(thickness)}
                row = {"name": f"{text(thickness)}x{text(width)}",
                       "select": {"Thickness": text(thickness), "Width": text(width)}}
                if profile:
                    # A profiled board's row (mitcad#477): every parameter of
                    # its profile, a cover width setting its tongue's length
                    # or its overlap; its board block and box.
                    params = board_parameters(dimensions)
                    params.update(sizes.get("profile") or {})
                    params.update(spec.get("profile") or {})
                    if covers is not None:
                        key = {"tongue_and_groove": "TL", "shiplap": "LW"}.get(profile)
                        if key is None:
                            fail(f"{what}: a {profile} board's cover follows from its overlap, not covers")
                        params[key] = actual - float(covers[i])
                    for name in ("W", "T", "L"):
                        params.pop(name, None)
                    for name, value in params.items():
                        angle = angle_value(value)
                        values[name] = angle[0] if angle else mm(length_of(value, f"{what}: {name}"))
                    env = board_numbers(dimensions, dict(params, W=actual, T=thickness, L=0))
                    check_board(dimensions, env, f"{what} {row['name']}")
                    build["board"] = board_block(dimensions, env)
                    _, box_w, box_h = board_shape(dimensions, env)
                    row["_box"] = [box_w, box_h]
                row.update({"values": values, "build": build})
                rows.append(row)
        elif shape == "fastener":
            diameter = float(spec["diameter"])
            head = spec.get("head") or []
            if len(head) != 2:
                fail(f"{what}: a fastener's sizes give the head as [diameter, height]")
            for length in spec.get("lengths") or []:
                length = float(length)
                build = {"wood": {"diameter": mm(diameter), "length": mm(length)}}
                if "availability" in spec:
                    build["availability"] = availability
                rows.append({"name": f"{diameter:.1f}x{text(length)}",
                             "select": {"Diameter": f"{diameter:.1f}", "Length": text(length)},
                             "values": {"D": mm(diameter), "L": mm(length), "DK": mm(float(head[0])),
                                        "K": mm(float(head[1]))}, "build": build})
        elif shape == "sheet":
            thickness = float(spec["thickness"])
            rows.append({"name": text(thickness), "select": {"Thickness": text(thickness)},
                         "values": {"T": mm(thickness)}, "build": {"availability": availability}})
        elif shape in TUBE_SHAPES:
            rows += tube_rows(entry, dimensions, sizes, spec, what)
        else:
            fail(f"{what}: sizes are made for a board, fastener, sheet or tube, not a {shape}")
    if shape == "board":
        # Thickness by thickness, each by its widths (the pickers' order).
        rows.sort(key=lambda r: (float(r["select"]["Thickness"]), float(r["select"]["Width"])))
    if shape in TUBE_SHAPES:
        # Size by size, each by its wall thicknesses.
        rows.sort(key=lambda r: tuple(float(n) for v in r["select"].values() for n in v.split("x")))
    entry["rows"] = rows
    names = [r["name"] for r in rows]
    if len(set(names)) != len(names):
        fail(f"{what}: sizes make a row twice")
    if entry.get("default") and entry["default"] not in names:
        fail(f"{what}: the default {entry['default']} is no row of its sizes")


def read_groups(folder, only):
    files = sorted(glob.glob(os.path.join(folder, "*.json")))
    if not files:
        fail(f"no group files (*.json) in {folder}")
    entries, categories, seen, left_out = [], {}, set(), []
    for path in files:
        name = os.path.basename(path)
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError) as e:
            fail(f"{name}: {e}")
        if isinstance(data, dict):
            for category in data.get("categories") or []:
                if not isinstance(category, dict) or "id" not in category:
                    fail(f"{name}: a category without an id")
                categories[category["id"]] = category.get("name") or category["id"]
            parts = data.get("parts", data.get("components"))
            # The supplier's files of the group that are no part, and why
            # (a duplicate, an assembly, a wrong file in a product's folder).
            notes = data.get("left_out") or []
            if not isinstance(notes, list) or not all(
                    isinstance(n, dict) and isinstance(n.get("source"), str) and isinstance(n.get("reason"), str)
                    for n in notes):
                fail(f"{name}: left_out is a list of {{\"source\", \"reason\"}}")
            left_out += [dict(n, _group=name) for n in notes]
            # The density (kg/m3) of the group's parts that name none.
            if data.get("density") is not None:
                for entry in data.get("parts", data.get("components")) or []:
                    if isinstance(entry, dict):
                        entry["_group_density"] = data["density"]
        else:
            parts = data
        if not isinstance(parts, list):
            fail(f"{name}: a group file is a list of parts, or an object with `parts`")
        for i, entry in enumerate(parts):
            if not isinstance(entry, dict):
                fail(f"{name}: part {i} is no object")
            for key in ("id", "name", "make"):
                if not isinstance(entry.get(key), str) or not entry[key].strip():
                    fail(f"{name}: part {i} has no {key}")
            if entry["make"] not in MAKES:
                fail(f"{name}: {entry['id']}: make '{entry['make']}' is none of {', '.join(MAKES)}")
            if entry["id"] in seen:
                fail(f"{name}: the id {entry['id']} is used twice")
            if entry["make"] != "model" and not entry.get("source"):
                fail(f"{name}: {entry['id']}: a {entry['make']} part names its source")
            if "build" in entry and not isinstance(entry["build"], dict):
                fail(f"{name}: {entry['id']}: build is no object")
            if "sizes" in entry:
                expand_sizes(entry, name)
            for row in entry.get("rows") or []:
                if isinstance(row, dict) and "source" in row and not isinstance(row["source"], str):
                    fail(f"{name}: {entry['id']}: the source of row {row.get('name')} is no path")
                if isinstance(row, dict) and "build" in row and not isinstance(row["build"], dict):
                    fail(f"{name}: {entry['id']}: the build of row {row.get('name')} is no object")
            seen.add(entry["id"])
            entry["_group"] = name
            if not only or entry["id"] in only:
                entries.append(entry)
    return entries, categories, left_out


def category_name(category, categories):
    return categories.get(category) or category.replace("-", " ").replace("_", " ").capitalize()


# The main groups of the library's categories (mitcad#527): the tree of
# groups the part pickers show, the main groups in this order. A category
# not named here stays a group at the top; a name with a slash is placed
# already.
MAIN_GROUPS = [
    ("Wood", ["sawn-timber", "planed-timber", "dimensioned-timber", "tongue-and-groove", "cladding", "decking",
              "sheets", "wood-fasteners", "timber-connectors"]),
    ("Slot profile", ["profiles", "connectors", "plates", "panels", "caps", "covers", "tubes", "accessories",
                      "slot-nuts", "slot-screws"]),
    ("Steel", ["steel-tubes"]),
    ("Motion", ["linear", "bearings", "motors"]),
    ("General fasteners", ["screws", "nuts", "washers"]),
    ("Hardware", ["handles", "feet", "castors"]),
    ("Workshop", ["workshop"]),
]


def grouped_categories(categories):
    """The manifest's categories with their main groups as path names
    (`Wood/Dimensioned timber`; a main group of its own name is that
    group), in the main groups' order, as the pickers show the groups in
    the manifest's order; the others after them as they were."""
    place = {}
    for rank, (main, ids) in enumerate(MAIN_GROUPS):
        for at, cid in enumerate(ids):
            place[cid] = (rank, at, main)
    out = []
    for category in categories:
        category = dict(category)
        rank, at, main = place.get(category["id"], (len(MAIN_GROUPS), 0, None))
        name = category.get("name") or category["id"]
        if main and "/" not in name and not category.get("parent") and name != main:
            category["name"] = f"{main}/{name}"
        out.append((rank, at, category))
    return [category for _, _, category in sorted(out, key=lambda item: (item[0], item[1]))]


MIT_TEXT = """MIT License

Copyright (c) 2026 Mitcad contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""


# Letters of the suppliers' file names written in ASCII in SOURCES.md.
ASCII_LETTERS = {"\u00e4": "ae", "\u00f6": "oe", "\u00fc": "ue", "\u00c4": "Ae", "\u00d6": "Oe", "\u00dc": "Ue",
                 "\u00df": "ss", "\u00b0": "deg"}


def ascii_text(text):
    return "".join(c if c.isascii() else ASCII_LETTERS.get(c, "?") for c in text)


def sources_text(components, entries, left_out=()):
    by_id = {e["id"]: e for e in entries}
    lines = ["# Sources of the parts", "",
             "The parts are Mitcad's own parametric designs. A profile's design is the",
             "cross-section of the supplier's CAD file extruded by its length; a connector,",
             "nut or other part keeps the solids of the supplier's CAD file; sheets and",
             "plates sold without a CAD file are modelled from the dimensions the supplier",
             "gives. Each part names its supplier and part number, so that it can be",
             "bought. Check critical dimensions against the supplier's data. The timber",
             "parts and the steel tubes name no supplier: their sizes come from the",
             "standards and tables named below.", ""]
    for component in components:
        entry = by_id[component["id"]]
        where = entry.get("source") or "the supplier's dimensions"
        if not entry.get("source") and entry.get("data_source"):
            # A part of no supplier (the timber parts): where its sizes
            # come from.
            where = entry["data_source"]
        if entry["make"] == "model" and entry.get("source"):
            where = f"modelled from the dimensions of {entry['source']}"
        url = entry.get("supplier_url") or ""
        supplier = " ".join(filter(None, [entry.get("supplier", ""), entry.get("part_number", "")]))
        line = f"- {component['id']} ({component['name']}): {where}"
        if supplier:
            line += f"; {supplier}"
        if url:
            line += f"; {url}"
        lines.append(line)
        # The supplier's files of the rows (another thread, colour or stock
        # length of the same part): the design is made from the part's.
        for row in entry.get("rows") or []:
            if isinstance(row, dict) and row.get("source"):
                row_line = f"  - {row['name']}: {row['source']}"
                if row.get("part_number"):
                    row_line += f"; {row['part_number']}"
                if row.get("supplier_url"):
                    row_line += f"; {row['supplier_url']}"
                lines.append(row_line)
    if left_out:
        lines += ["", "The supplier's files not made into parts, and why:", ""]
        lines += [f"- {n['source']}: {n['reason']}" for n in left_out]
    lines += ["", "The generator: tools/libraries/make-profile-library.py in Mitcad's repository.", ""]
    return ascii_text("\n".join(lines))


class Context:
    pass


FASTENER_WORDS = ("nut", "screw", "bolt", "washer", "nail", "fastener", "rivet")


def is_fastener(entry):
    """A fastener (mitcad#526: rendered in the studio, with its finish): a
    BUILD fastener, or a part of a category of nuts, screws, washers and
    the like."""
    build = entry.get("build") if isinstance(entry.get("build"), dict) else {}
    category = str(entry.get("category") or "").lower().replace("_", "-")
    return build.get("role") == "fastener" or any(
        word in part for part in category.replace("/", "-").split("-") for word in FASTENER_WORDS)


def studio_preview(ctx, design, entry):
    """A fastener's picture rendered in the studio (library_previews), the
    library's picture kept when its design did not change; None when it
    cannot be rendered (the caller draws one)."""
    if not ctx.studio:
        return None
    old = previews.read_old(os.path.join(ctx.out, "previews", f"{entry['id']}.png"))
    # `preview.up`: the part's direction that is up in the picture, where
    # its mass does not tell its head (library_previews.part_axis).
    setup = entry.get("preview")
    up = setup.get("up") if isinstance(setup, dict) else None
    try:
        image, how = previews.render_preview(ctx.cli, design, ctx.scratch, ctx.worker, up=up, old=old,
                                             force=ctx.rerender, threads=entry.get("_threads"))
    except previews.PreviewError as e:
        previews.warn(f"{entry['id']}: the preview was not rendered ({e}); drawn instead")
        return None
    print(f"preview {entry['id']}: {how} ({entry.get('appearance')})")
    if how.startswith("rendered without"):
        previews.PROBLEMS.append(f"{entry['id']}: {how}")
    return image


def make_part(ctx, mcp, entry):
    if is_fastener(entry) and not entry.get("appearance"):
        # A fastener without a finish gets one by what its name says
        # (stainless, brass), else zinc plated steel.
        entry["appearance"] = previews.default_finish(entry.get("name"), entry.get("part_number"), entry["id"])
    build = copy.deepcopy(entry.get("build") or {})
    # The rows' own build blocks (a timber size's section, species and
    # availability): the manifest's build.rows, merged over the part's by
    # whoever reads a row; the part's own fields are its default row's.
    row_builds = {row["name"]: row["build"] for row in entry.get("rows") or []
                  if isinstance(row, dict) and isinstance(row.get("build"), dict)}
    if row_builds:
        default = entry.get("default") or entry["rows"][0]["name"]
        build = merged_block(build, row_builds.get(default, {}))
        build["rows"] = row_builds
    density = density_of(entry)
    if density is not None and isinstance(build.get("material"), dict):
        build["material"].setdefault("density", int(density) if density.is_integer() else density)
    if entry["make"] == "section":
        design, made = make_section(ctx, mcp, entry, build)
    elif entry["make"] == "solid":
        design, made = make_solid(ctx, mcp, entry, build)
    else:
        design, made = make_model(ctx, mcp, entry, build)
    category = entry.get("category") or build.get("role") or "parts"
    path = f"{category}/{entry['id']}.mitcad"
    target = os.path.join(ctx.out, path)
    os.makedirs(os.path.dirname(target), exist_ok=True)
    shutil.copyfile(design, target)
    os.makedirs(os.path.join(ctx.out, "previews"), exist_ok=True)
    # A timber board's rows each from its end (mitcad#484); the part's
    # picture its default row's.
    by_row = row_previews(ctx, mcp, design, entry)
    # A fastener in the studio (mitcad#526): one picture per part, its rows
    # differing in size only.
    image = studio_preview(ctx, design, entry) if is_fastener(entry) and not by_row else None
    if by_row:
        image = by_row.get(entry.get("default") or entry["rows"][0]["name"])
    elif image is None:
        image = preview(ctx, design, made, design_bounds(mcp, ctx, design) if made["kind"] != "section" else None)
    preview_path = f"previews/{entry['id']}.png"
    with open(os.path.join(ctx.out, preview_path), "wb") as f:
        f.write(image)
    component = {"id": entry["id"], "path": path, "category": category, "name": entry["name"]}
    for key in ("standard", "description", "keywords", "license", "designation", "supplier", "part_number",
                "supplier_url"):
        if entry.get(key):
            component[key] = entry[key]
    component["preview"] = preview_path
    if by_row:
        component["row_previews"] = {}
        for row, data in by_row.items():
            row_path = f"previews/{entry['id']}-{row}.png"
            with open(os.path.join(ctx.out, row_path), "wb") as f:
                f.write(data)
            component["row_previews"][row] = row_path
    # A plain part (no build block in its group file) stays one: the frames
    # the making found are not a BUILD part's.
    if build and entry.get("build"):
        component["build"] = build
    rows = entry.get("rows") or []
    if rows and "description" not in component:
        component["description"] = f"{len(rows)} sizes, {rows[0]['name']} to {rows[-1]['name']}."
    return component


def kept_fields(out, fields):
    """The `fields` of the manifest in `out` that it has (mitcad#949: the
    fasteners' `suppliers`), for --merge."""
    try:
        with open(os.path.join(out, "mitcad-library.json"), encoding="utf-8") as f:
            old = json.load(f)
    except (OSError, ValueError):
        return {}
    if not isinstance(old, dict):
        return {}
    return {field: old[field] for field in fields if old.get(field)}


def merged(out, components, categories, sources):
    """This run's components, categories and sources, then those of the
    manifest in `out` that this run does not make (as
    make-fastener-library.py's --merge)."""
    path = os.path.join(out, "mitcad-library.json")
    try:
        with open(path, encoding="utf-8") as f:
            old = json.load(f)
    except (OSError, ValueError):
        return components, categories, sources
    if not isinstance(old, dict):
        return components, categories, sources
    made = {c["id"] for c in components}
    kept = [c for c in old.get("components") or [] if isinstance(c, dict) and c.get("id") not in made]
    used = {c.get("category") for c in kept}
    names = {c["id"] for c in categories}
    more = [c for c in old.get("categories") or []
            if isinstance(c, dict) and c.get("id") in used and c.get("id") not in names]
    extra = [s for s in old.get("sources") or [] if s not in sources]
    return components + kept, categories + more, sources + extra


def inside_project(folder):
    folder = os.path.abspath(folder)
    while True:
        if os.path.isfile(os.path.join(folder, ".mitcad", "project.json")):
            return True
        parent = os.path.dirname(folder)
        if parent == folder:
            return False
        folder = parent


def main():
    # The parts' source paths are the supplier's UTF-8 file names; a console
    # in another code page must not stop the build.
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", required=True, help="mitcad-cli")
    parser.add_argument("--source", required=True, help="the folder of the supplier's files (never copied)")
    parser.add_argument("--out", required=True, help="the library's folder (made when missing)")
    parser.add_argument("--groups", default=os.path.join(HERE, "profiles"), help="the folder of the group files")
    parser.add_argument("--only", default="", help="only these part ids, e.g. p30x30-b8,tnut-b8-m6")
    parser.add_argument("--version", default="1.0.0", help="the library's version")
    parser.add_argument("--commit", action="store_true", help="record the folder as a git version and tag it")
    parser.add_argument("--work", help="folder for intermediate files (default: next to --out)")
    parser.add_argument("--no-render", action="store_true",
                        help="draw the previews of solids from their bounds instead of rendering them")
    parser.add_argument("--merge", action="store_true",
                        help="keep the library's other parts of the manifest in --out (the fastener generator's)")
    parser.add_argument("--render-worker",
                        help="mitcad-render for the previews (default: MITCAD_RENDER_WORKER, else next to --cli)")
    parser.add_argument("--rerender", action="store_true",
                        help="render every fastener's preview, also those whose design did not change")
    parser.add_argument("--strict", action="store_true",
                        help="fail (after making the library) when a fastener's preview was drawn instead of "
                             "rendered, rendered without its thread, or a screw's thread or rounding was left out")
    args = parser.parse_args()

    only = {s.strip() for s in args.only.split(",") if s.strip()}
    entries, categories, left_out_files = read_groups(args.groups, only)
    if not entries:
        fail("no parts to make")
    ctx = Context()
    ctx.cli = os.path.abspath(args.cli) if os.path.exists(args.cli) else args.cli
    ctx.source = os.path.abspath(args.source)
    ctx.out = os.path.abspath(args.out)
    ctx.render = not args.no_render
    ctx.worker = os.path.abspath(args.render_worker) if args.render_worker else None
    ctx.rerender = args.rerender
    # The fasteners' studio (mitcad#526) when a render worker is there.
    ctx.studio = False
    if ctx.render and any(is_fastener(entry) for entry in entries):
        ctx.studio, reason = previews.renderer_available(ctx.cli, ctx.worker)
        if not ctx.studio:
            print(f"warning: no render worker ({reason}); the fasteners' previews are drawn")
            previews.PROBLEMS.append(f"no render worker ({reason})")
    # The files the group files name and do not make: a note on one that
    # is not in --source (moved or renamed by the supplier).
    for entry in entries:
        for row in entry.get("rows") or []:
            source = row.get("source") if isinstance(row, dict) else None
            if source and not os.path.isfile(os.path.join(ctx.source, source)):
                print(f"warning {entry['id']}: the source of row {row.get('name')}, {source}, is not in --source")
    for note in left_out_files:
        if not os.path.isfile(os.path.join(ctx.source, note["source"])):
            print(f"warning {note['_group']}: the left-out file {note['source']} is not in --source")
    os.makedirs(ctx.out, exist_ok=True)
    if not os.path.isfile(os.path.join(ctx.out, ".mitcad", "project.json")):
        run([ctx.cli, "project", "init", ctx.out, "--no-history"])
    work = os.path.abspath(args.work or os.path.dirname(ctx.out))
    os.makedirs(work, exist_ok=True)
    components, left_out = [], []
    with tempfile.TemporaryDirectory(prefix=".profile-work-", dir=work) as scratch:
        ctx.scratch = scratch
        if inside_project(scratch):
            fail(f"--work {work} is inside a Mitcad project; give a folder outside it")
        # The blank design each made part starts from (Mcp.create).
        write(os.path.join(scratch, "blank.json"), "[]\n")
        try:
            run([ctx.cli, "run", os.path.join(scratch, "blank.json"), "--save", os.path.join(scratch, Mcp.BLANK)])
        except PartError as e:
            fail(str(e))
        mcp = Mcp(ctx.cli, scratch)
        try:
            for entry in entries:
                try:
                    components.append(make_part(ctx, mcp, entry))
                    print(f"made {entry['id']}")
                except PartError as e:
                    # One line per part, whatever the reason holds.
                    reason = " | ".join(line.strip() for line in str(e).splitlines() if line.strip())
                    left_out.append((entry["id"], reason))
                    print(f"left out {entry['id']}: {reason}", flush=True)
        finally:
            mcp.finish()
    if not components:
        fail("no part could be made")
    made = components
    used = []
    for component in components:
        if component["category"] not in used:
            used.append(component["category"])
    library_categories = [{"id": c, "name": category_name(c, categories)} for c in used]
    sources = ["The suppliers' CAD files and dimensions, made parametric; see SOURCES.md"]
    if args.merge:
        # The library's other parts (the fasteners) stay.
        components, library_categories, sources = merged(ctx.out, components, library_categories, sources)
    library_categories = grouped_categories(library_categories)
    manifest = {
        "format": "mitcad-library", "version": 1, "id": LIBRARY_ID, "name": LIBRARY_NAME,
        "description": LIBRARY_DESCRIPTION,
        "library_version": args.version, "license": LICENSE, "authors": ["Mitcad contributors"],
        "sources": sources, "units": "mm",
        "categories": library_categories,
        "components": components,
    }
    if args.merge:
        # The fasteners' suppliers and the ids the library replaces
        # (mitcad#949) stay.
        manifest.update(kept_fields(ctx.out, ("suppliers", "replaces")))
    write(os.path.join(ctx.out, "mitcad-library.json"), json.dumps(manifest, indent=2) + "\n")
    write(os.path.join(ctx.out, "LICENSE"), MIT_TEXT)
    write(os.path.join(ctx.out, "SOURCES.md"), sources_text(made, entries, [] if only else left_out_files))
    write(os.path.join(ctx.out, "README.md"),
          f"# {LIBRARY_NAME}\n\n"
          "Slot profiles, connectors, nuts, caps, panels and linear parts for Mitcad's BUILD tab.\n"
          "Each part is a parametric design (a profile is cut to any length) and names its supplier\n"
          "and part number. In Mitcad: Libraries, add this repository's URL, Fetch; then the BUILD\n"
          "tab's pickers list the parts. Designs record the version they use.\n\n"
          f"Licence: {LICENSE} (LICENSE). Where the parts come from: SOURCES.md.\n")
    check = run([ctx.cli, "library", "check", ctx.out], check=False)
    sys.stdout.write(check.stdout + check.stderr)
    if check.returncode != 0:
        fail("the library check found errors; nothing was recorded")
    if args.commit:
        identity = ["-c", "user.name=Mitcad developers", "-c", "user.email=devs@mitcad.org"]
        if not os.path.isdir(os.path.join(ctx.out, ".git")):
            run(["git", "init", "-q", "-b", "main", ctx.out])
        run(["git", "-C", ctx.out, "add", "-A"])
        status = run(["git", "-C", ctx.out, "status", "--porcelain"]).stdout
        if status.strip():
            run(["git", "-C", ctx.out] + identity + ["commit", "-q", "-m", f"{LIBRARY_NAME} {args.version}"])
        run(["git", "-C", ctx.out] + identity + ["tag", "-a", "-f", f"v{args.version}", "-m",
                                                  f"{LIBRARY_NAME} {args.version}"])
    print(f"{LIBRARY_ID} {args.version}: {len(made)} components in {ctx.out}")
    if left_out:
        print(f"left out: {len(left_out)} ({', '.join(i for i, _ in left_out)})")
    previews.strict_exit(args.strict, "make-profile-library")


if __name__ == "__main__":
    try:
        main()
    except PartError as e:
        fail(str(e))
