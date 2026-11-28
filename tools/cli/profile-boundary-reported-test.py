# SPDX-License-Identifier: MIT
"""Profile Boundary on the reported design of GitHub mitcad/Mitcad#712
(mitcad#2596, a follow-up of mitcad#2551; never committed).

The report's archive is in `github-712/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the design
in it by its own, copied into the work folder once for each case. The
design: a 70 x 50 x 6 plate with 4 mm rounded corners (Body1, Extrude1 of
Sketch1), four bosses of 12 mm diameter and 3 mm height standing on its top
face 20 mm and 10 mm off its middle (Extrude2 of Sketch2's circles, their
diameter the parameter d10, the right pair's distance to the plate's right
edge d9), their tops chamfered (Chamfer1). Sketch3 (F8) lies on the top
face with the face's outline it got when it was made (the automatic linked
projection: the four lines, the four corner arcs and the four bosses'
circles) and the outline offset 2 mm inwards (d11). That inset is one
profile with the circles inside it, as the reporter found: an Extrude of it
fills the bosses' footprints.

mitcad-cli mcp opens copies of the design in the work folder, chooses
circles with `sketch.set_profile_boundary`, reads the regions with the
`sketch` and `profiles` queries (a point in the sketch picks the region
holding it, as a click does) and saves; `mitcad-cli run` previews the
Extrude of the inset as the Extrude panel does and commits it (the preview
step fails the run unless the preview is ok; the commit takes the preview's
result), saving the design, which mitcad-cli mcp opens again to measure the
new body and its holes:

- all four circles chosen: the inset has four holes and each circle is a
  profile of its own; the Extrude (5 mm, a new body) leaves all four
  footprints clear: (inset - 4 discs) x 5 mm, a hole on each boss's axis;
- saved and opened again, the choice holds; d10 12 -> 10 mm makes every
  hole 10 mm across, and d9 15 -> 12 mm moves the right pair 3 mm outwards,
  their holes with them;
- two circles chosen: only those two footprints are clear;
- the Offset's curves removed: the face-bounded area (the face less the
  footprints) is one profile before and after the four circles are chosen,
  with the same key and area, and extrudes as such;
- without a choice, the Extrude of the inset keeps its whole area, saved
  and opened again, and Extrude1's and Extrude2's saved profiles keep their
  areas (Body1's volume stays the design's in every case).

Skipped when the folder or the archive is not there.

  python3 profile-boundary-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import math
import os
import queue
import subprocess
import sys
import threading
import zipfile

ARCHIVE = "ec31abf8"  # the reporter's archive
DESIGN = "e5b7f8d6"  # the design in it
FOLDER = "github-712"
SKIP = "profile boundary reported: skipped"
TIMEOUT = 600
PROTOCOL = "2025-11-25"

SKETCH = "F8"  # Sketch3, on the plate's top face
BODY = "F2.b0"  # Body1
# The bosses' centres in Sketch3's coordinates, and their radius.
BOSSES = [(-20.0, -10.0), (20.0, -10.0), (-20.0, 10.0), (20.0, 10.0)]
RADIUS = 6.0
INSIDE = (0.0, 0.0)  # in the inset, between the bosses
HEIGHT = 5.0  # the Extrude's distance
# The inset: 66 x 46 with 2 mm rounded corners; the face: 70 x 50 with
# 4 mm ones.
INSET = 66.0 * 46.0 - 4.0 * (4.0 - math.pi)
FACE = 70.0 * 50.0 - 4.0 * (16.0 - 4.0 * math.pi)
REGION_TOLERANCE = 1e-3  # relative, of the sketch's region areas
VOLUME_TOLERANCE = 1e-6  # relative, of the bodies
PLACE_TOLERANCE = 1e-3  # mm, a hole's axis on its boss's centre


def disc(radius=RADIUS):
    return math.pi * radius * radius


def content_id(data):
    return hashlib.sha256(data).hexdigest()[:8]


def find_design():
    """The design's bytes from the reporter's archive, or None."""
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, FOLDER)
    if not reported or not os.path.isdir(folder):
        print(f"{SKIP} (MITCAD_MITCAD_REPORTED/{FOLDER} is not there)")
        return None
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if not (name.endswith(".zip") and os.path.isfile(path)):
            continue
        with open(path, "rb") as f:
            if content_id(f.read()) != ARCHIVE:
                continue
        with zipfile.ZipFile(path) as archive:
            for entry in archive.infolist():
                if not entry.filename.endswith(".mitcad"):
                    continue
                data = archive.read(entry)
                if content_id(data) == DESIGN:
                    return data
        sys.exit(f"FAIL: the archive {ARCHIVE} holds no design {DESIGN}")
    print(f"{SKIP} (no archive {ARCHIVE} in {folder})")
    return None


class Session:
    """mitcad-cli mcp over stdin and stdout, a reader thread for timeouts."""

    def __init__(self, cli, workspace):
        self.stderr = open(os.path.join(workspace, "mcp-stderr.txt"), "wb")
        self.process = subprocess.Popen([cli, "mcp", "--workspace", workspace],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr)
        self.messages = queue.Queue()
        self.next_id = 0
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        while True:
            line = self.process.stdout.readline()
            self.messages.put(line)
            if not line:
                return

    def request(self, method, params):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}
        self.process.stdin.write(json.dumps(message).encode("utf-8") + b"\n")
        self.process.stdin.flush()
        try:
            line = self.messages.get(timeout=TIMEOUT)
        except queue.Empty:
            sys.exit(f"FAIL: {method} timed out")
        if not line:
            sys.exit(f"FAIL: mitcad-cli mcp ended at {method}")
        response = json.loads(line)
        if response.get("id") != self.next_id or "error" in response:
            sys.exit(f"FAIL: {method}: {response}")
        return response["result"]

    def notify(self, method):
        message = {"jsonrpc": "2.0", "method": method}
        self.process.stdin.write(json.dumps(message).encode("utf-8") + b"\n")
        self.process.stdin.flush()

    def tool(self, name, arguments):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        content = result.get("structuredContent", {})
        if result.get("isError"):
            sys.exit(f"FAIL: {name} {json.dumps(arguments)}: {content}")
        return content

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.stderr.close()


def check(condition, message):
    if not condition:
        sys.exit(f"FAIL: {message}")


def close(actual, expected, tolerance):
    return abs(actual - expected) <= tolerance * max(abs(expected), 1.0)


class Design:
    """One design open in the session."""

    def __init__(self, session, path):
        self.session = session
        self.path = path
        self.document = session.tool("document_open", {"path": path})["document"]

    def query(self, request):
        return self.session.tool("model_query", {"document": self.document,
                                                 "query": request})["result"]

    def command(self, command):
        return self.session.tool("model_command", {"document": self.document,
                                                   "command": command})

    def save(self, path):
        # A run before this one left its files in the work folder.
        self.session.tool("document_save", {"document": self.document, "path": path,
                                            "overwrite": True})

    def sketch(self):
        return self.query({"query": "sketch", "uid": SKETCH})

    def outline(self):
        """Sketch3's automatic projection (the face's outline)."""
        listed = [p for p in self.sketch().get("projections", []) if p.get("automatic")]
        check(len(listed) == 1, f"Sketch3 has {len(listed)} automatic outlines")
        return listed[0]

    def circles(self):
        """The outline's circles: {id: ((u, v), radius)}."""
        s = self.sketch()
        listed = set(self.outline()["entities"])
        found = {}
        for entity in s["entities"]:
            if entity.get("type") == "circle" and entity["id"] in listed:
                g = entity["geometry"]
                found[entity["id"]] = ((g["center"][0], g["center"][1]), g["radius"])
        return found

    def circle_at(self, center):
        for curve, (at, _) in self.circles().items():
            if math.dist(at, center) < PLACE_TOLERANCE:
                return curve
        sys.exit(f"FAIL: no circle of the outline about {center}: {self.circles()}")

    def region(self, key):
        found = [r for r in self.sketch()["regions"] if r["key"] == key]
        check(len(found) == 1, f"Sketch3 has no region {key}")
        return found[0]

    def picked(self, point):
        """The region a click at `point` (Sketch3's coordinates) picks."""
        found = self.query({"query": "profiles", "sketch": SKETCH, "point": list(point)})
        check(len(found) == 1, f"a pick at {point} finds {len(found)} regions: {found}")
        return found[0]["region"]

    def choose(self, curves, boundary=True):
        self.command({"cmd": "sketch.set_profile_boundary", "sketch": SKETCH,
                      "curves": curves, "boundary": boundary})

    def chosen(self):
        return sorted(self.outline().get("boundaries", []))

    def body_volume(self, uid):
        for body in self.query({"query": "bodies", "properties": True}):
            if body["uid"] == uid:
                return body["volume"]
        sys.exit(f"FAIL: no body {uid}")

    def new_body(self):
        """The body other than Body1, as (uid, volume)."""
        bodies = self.query({"query": "bodies", "properties": True})
        others = [b for b in bodies if b["uid"] != BODY]
        check(len(bodies) == 2 and len(others) == 1,
              f"expected Body1 and the Extrude's body: {[b['uid'] for b in bodies]}")
        return others[0]["uid"], others[0]["volume"]

    def holes(self, uid, radius):
        """The axes (x, y) of the body's cylindrical faces of `radius`."""
        faces = self.query({"query": "faces", "body": uid, "geometry": True,
                            "surface": "cylinder"})
        axes = []
        for face in faces["faces"]:
            if abs(face["radius"] - radius) > PLACE_TOLERANCE:
                continue
            at = (face["origin"][0], face["origin"][1])
            if not any(math.dist(at, known) < PLACE_TOLERANCE for known in axes):
                axes.append(at)
        return sorted(axes)

    def in_design(self, points):
        """Sketch3's points as (x, y) of the design."""
        answer = self.query({"query": "sketch_surface", "sketch": SKETCH,
                             "points": [list(p) for p in points]})
        return sorted((p["at"][0], p["at"][1]) for p in answer["points"])


def same_places(actual, expected):
    """Whether each expected place has its own actual place within the
    tolerance, none left over."""
    left = list(actual)
    for place in expected:
        near = [a for a in left if math.dist(a, place) < PLACE_TOLERANCE]
        if not near:
            return False
        left.remove(near[0])
    return not left


class Test:
    def __init__(self, cli, work, data):
        self.cli = cli
        self.work = work
        self.data = data
        self.session = Session(cli, work)
        self.session.request("initialize", {
            "protocolVersion": PROTOCOL, "capabilities": {},
            "clientInfo": {"name": "mitcad-profile-boundary-test", "version": "1"}})
        self.session.notify("notifications/initialized")
        self.body_volume = None

    def copy(self, name):
        """A copy of the reported design in the work folder, opened."""
        with open(os.path.join(self.work, name), "wb") as out:
            out.write(self.data)
        return Design(self.session, name)

    def extrude(self, design, key, name):
        """Saves `design`, previews and commits the Extrude of `key` (a new
        body, HEIGHT up) with mitcad-cli run as the Extrude panel does, and
        opens the result."""
        design.save(f"{name}-saved.mitcad")
        command = {"cmd": "add_feature", "def": {
            "type": "extrude", "profiles": [{"sketch": SKETCH, "region": key}],
            "extent": {"type": "distance", "distance": HEIGHT}, "operation": "new_body"}}
        script = [
            {"comment": "SPDX-License-Identifier: MIT. Written by "
                        "profile-boundary-reported-test.py."},
            {"preview": command},
            command,
            {"expect": {"bodies": 2}},
            {"expect": {"body": BODY, "volume": self.body_volume, "tolerance": VOLUME_TOLERANCE}},
        ]
        path = os.path.join(self.work, f"{name}-extrude.json")
        with open(path, "w", encoding="utf-8") as out:
            json.dump(script, out, indent=1)
        result = subprocess.run(
            [self.cli, "run", path, "--open", os.path.join(self.work, f"{name}-saved.mitcad"),
             "--save", os.path.join(self.work, f"{name}-extruded.mitcad")],
            capture_output=True, timeout=TIMEOUT)
        check(result.returncode == 0,
              f"{name}: the Extrude's preview or commit failed ({result.returncode}):\n"
              f"{result.stdout.decode(errors='replace')}{result.stderr.decode(errors='replace')}")
        extruded = Design(self.session, f"{name}-extruded.mitcad")
        check(close(extruded.body_volume(BODY), self.body_volume, VOLUME_TOLERANCE),
              f"{name}: Body1's volume changed: {extruded.body_volume(BODY)}, "
              f"not {self.body_volume}")
        return extruded

    def expect_extrude(self, design, name, area, centers, radius=RADIUS):
        """The Extrude's body: `area` x HEIGHT, a hole on each of `centers`
        (Sketch3's coordinates) of `radius`."""
        uid, volume = design.new_body()
        expected = area * HEIGHT
        check(close(volume, expected, VOLUME_TOLERANCE),
              f"{name}: the Extrude's body has volume {volume}, not {expected}")
        axes = design.holes(uid, radius)
        wanted = design.in_design(centers) if centers else []
        check(same_places(axes, wanted),
              f"{name}: the Extrude's holes of radius {radius} are at {axes}, not at {wanted}")
        print(f"ok   {name}: the Extrude's body is {volume:.4f} mm3 with {len(axes)} holes "
              f"of {2 * radius:g} mm on the bosses' axes")

    def inset(self, design):
        """The inset's region key: the one a click between the bosses picks,
        bounded by the Offset's curves."""
        s = design.sketch()
        offsets = s.get("offsets", [])
        check(len(offsets) == 1, f"Sketch3 has {len(offsets)} offsets")
        key = design.picked(INSIDE)
        check(all(f"{c}[" in key for c in offsets[0]["results"]),
              f"the region between the bosses, {key}, is not the inset "
              f"(the Offset's curves {offsets[0]['results']})")
        return key

    def saved_profiles(self, design):
        """Extrude1's and Extrude2's saved profiles keep their areas."""
        for uid, area in (("F2", FACE), ("F4", disc())):
            feature = design.query({"query": "feature", "uid": uid})
            for profile in feature["def"]["profiles"]:
                found = design.query({"query": "profile", "sketch": profile["sketch"],
                                      "region": profile["region"]})
                check(found is not None and found.get("exact"),
                      f"{uid}'s profile {profile['region']} is no longer its "
                      f"sketch's own region: {found}")
                sketch = design.query({"query": "sketch", "uid": profile["sketch"]})
                region = [r for r in sketch["regions"] if r["key"] == profile["region"]]
                check(len(region) == 1 and close(region[0]["area"], area, REGION_TOLERANCE),
                      f"{uid}'s profile {profile['region']}: {region}, not of area {area}")

    def run(self):
        # As reported: the inset is one profile across the bosses' circles.
        plain = self.copy("plain.mitcad")
        self.body_volume = plain.body_volume(BODY)
        print(f"     Body1: {self.body_volume:.4f} mm3")
        self.saved_profiles(plain)
        circles = plain.circles()
        check(len(circles) == 4 and all(abs(r - RADIUS) < PLACE_TOLERANCE
                                        for _, r in circles.values()),
              f"Sketch3's outline has not the four bosses' circles: {circles}")
        check(same_places([at for at, _ in circles.values()], BOSSES),
              f"the outline's circles are at {circles}, not at {BOSSES}")
        key = self.inset(plain)
        region = plain.region(key)
        check(len(region["loops"]) == 1 and close(region["area"], INSET, REGION_TOLERANCE),
              f"before a choice the inset is not one area without holes: {region}")
        for center in BOSSES:
            check(plain.picked(center) == key,
                  f"before a choice a pick on the boss at {center} does not take the inset")
        print(f"ok   as reported: the inset {key} is one profile of {region['area']:.4f} mm2 "
              f"across the four circles")
        # Without a choice the saved Extrude of the inset keeps its area.
        extruded = self.extrude(plain, key, "plain")
        self.expect_extrude(extruded, "no choice", INSET, [])
        check(not extruded.chosen(), "the design without a choice reads one")
        self.saved_profiles(extruded)
        print("ok   saved Extrude profiles keep their areas")

        # All four circles chosen.
        four = self.copy("four.mitcad")
        ids = [four.circle_at(c) for c in BOSSES]
        four.choose(ids)
        check(four.chosen() == sorted(ids), f"chosen {four.chosen()}, not {sorted(ids)}")
        check(self.inset(four) == key, "the inset's key changed with the choice")
        region = four.region(key)
        expected = INSET - 4 * disc()
        check(len(region["loops"]) == 5 and close(region["area"], expected, REGION_TOLERANCE),
              f"with four circles chosen the inset is not {expected} mm2 with four holes: {region}")
        for curve, center in zip(ids, BOSSES):
            picked = four.picked(center)
            check(picked == f"r{{{curve}}}",
                  f"a pick on the boss at {center} takes {picked}, not the circle's disc")
            check(close(four.region(picked)["area"], disc(), REGION_TOLERANCE),
                  f"the disc {picked} is not {disc()} mm2")
        print(f"ok   four circles chosen: the inset has four holes ({region['area']:.4f} mm2), "
              f"each circle a profile of its own")
        extruded = self.extrude(four, key, "four")
        self.expect_extrude(extruded, "four circles", expected, BOSSES)
        # Opened again (the run saved it): the choice holds.
        check(extruded.chosen() == sorted(ids),
              f"opened again, the choice is {extruded.chosen()}, not {sorted(ids)}")
        print("ok   saved and opened again, the four circles stay chosen")
        # A boss's diameter: every hole follows.
        extruded.command({"cmd": "set_parameter", "name": "d10", "value": 10})
        circles = extruded.circles()
        check(sorted(circles) == sorted(ids) and all(abs(r - 5.0) < PLACE_TOLERANCE
                                                     for _, r in circles.values()),
              f"d10 = 10 mm: the outline's circles are {circles}")
        check(extruded.chosen() == sorted(ids), f"d10 = 10 mm: chosen {extruded.chosen()}")
        self.expect_extrude(extruded, "bosses 10 mm across", INSET - 4 * disc(5.0), BOSSES, 5.0)
        # The right pair 3 mm outwards: their holes move with them.
        extruded.command({"cmd": "set_parameter", "name": "d9", "value": 12})
        moved = [at for at, _ in extruded.circles().values()]
        right = [at for at in moved if abs(abs(at[0]) - 23.0) < PLACE_TOLERANCE]
        left = [at for at in moved if abs(abs(at[0]) - 20.0) < PLACE_TOLERANCE]
        check(len(right) == 2 and len(left) == 2 and right[0][0] * right[1][0] > 0,
              f"d9 = 12 mm: the outline's circles are at {moved}")
        check(len(extruded.chosen()) == 4 and set(extruded.chosen()) <= set(extruded.circles()),
              f"d9 = 12 mm: chosen {extruded.chosen()} of {extruded.circles()}")
        self.expect_extrude(extruded, "the right pair moved", INSET - 4 * disc(5.0), moved, 5.0)

        # Two circles chosen: only those two footprints clear.
        two = self.copy("two.mitcad")
        pair = [c for c in BOSSES if c[0] > 0]
        others = [c for c in BOSSES if c[0] < 0]
        two.choose([two.circle_at(c) for c in pair])
        region = two.region(key)
        expected = INSET - 2 * disc()
        check(len(region["loops"]) == 3 and close(region["area"], expected, REGION_TOLERANCE),
              f"with two circles chosen the inset is not {expected} mm2 with two holes: {region}")
        for center in pair:
            check(two.picked(center) == f"r{{{two.circle_at(center)}}}",
                  f"a pick on the chosen boss at {center} does not take its disc")
        for center in others:
            check(two.picked(center) == key,
                  f"a pick on the boss at {center}, not chosen, does not take the inset")
        print(f"ok   two circles chosen: the inset has two holes ({region['area']:.4f} mm2)")
        extruded = self.extrude(two, key, "two")
        self.expect_extrude(extruded, "two circles", expected, pair)

        # Without the Offset: the face-bounded area stays one profile.
        bare = self.copy("bare.mitcad")
        offset = bare.sketch()["offsets"][0]
        bare.command({"cmd": "sketch.remove", "sketch": SKETCH,
                      "items": offset["results"] + offset.get("helpers", [])})
        check(not bare.sketch().get("offsets"), "the Offset is still there")
        face = bare.picked(INSIDE)
        expected = FACE - 4 * disc()
        check(close(bare.region(face)["area"], expected, REGION_TOLERANCE),
              f"without the Offset the area between the bosses is not the face less the "
              f"footprints ({expected} mm2): {bare.region(face)}")
        bare.choose([bare.circle_at(c) for c in BOSSES])
        check(bare.picked(INSIDE) == face,
              f"with the circles chosen the face-bounded area {face} became {bare.picked(INSIDE)}")
        check(close(bare.region(face)["area"], expected, REGION_TOLERANCE),
              f"with the circles chosen the face-bounded area is {bare.region(face)}")
        print(f"ok   without the Offset the face-bounded area {face} stays one profile "
              f"of {expected:.4f} mm2")
        extruded = self.extrude(bare, face, "bare")
        self.expect_extrude(extruded, "the face-bounded area", expected, BOSSES)
        self.session.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    data = find_design()
    if data is None:
        return 0
    work = os.path.abspath(args.work)
    os.makedirs(work, exist_ok=True)
    Test(args.cli, work, data).run()
    return 0


if __name__ == "__main__":
    sys.exit(main())
