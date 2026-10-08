# SPDX-License-Identifier: MIT
"""Builds Mitcad's library of metric fasteners (mitcad#64) from the
standards' dimension tables: one Mitcad design per standard, made with
mitcad-cli from commands, with a configuration table of its sizes; preview
images; the library's manifest, licence note, sources and README. With
--commit the folder becomes a git repository with the version recorded and
tagged.

    python3 make-fastener-library.py --cli build/dev/tools/cli/mitcad-cli \
        --out ../libraries/mitcad-fasteners [--sizes M3,M5] \
        [--standards iso4762,iso4032] [--version 1.0.0] [--commit]

The geometry is simplified for assemblies: heads, shanks, sockets and hex
flats at their nominal sizes, no threads, chamfers or fillets. Lengths are
measured as the standards do (ISO 10642 includes the head).
"""

import argparse
import json
import math
import os
import struct
import subprocess
import sys
import tempfile
import zlib

LIBRARY_ID = "mitcad-fasteners"
LICENSE = "CC0-1.0"

# Dimension tables, millimetres. Head and nut sizes are the standards'
# maximum (nominal) values, socket sizes the nominal key sizes, socket
# depths the minimum depths, washer holes their minimum diameters. The
# values are the standards' facts, tabulated for this library.
SIZES = ["M3", "M4", "M5", "M6", "M8", "M10", "M12"]
DIAMETER = {"M3": 3, "M4": 4, "M5": 5, "M6": 6, "M8": 8, "M10": 10, "M12": 12}
PITCH = {"M3": 0.5, "M4": 0.7, "M5": 0.8, "M6": 1, "M8": 1.25, "M10": 1.5, "M12": 1.75}

# ISO 4762 hexagon socket head cap screws: head diameter dk, head height
# k, socket size s, socket depth t; preferred lengths.
ISO4762 = {
    "M3": (5.5, 3, 2.5, 1.3, [5, 6, 8, 10, 12, 16, 20, 25, 30]),
    "M4": (7, 4, 3, 2, [6, 8, 10, 12, 16, 20, 25, 30, 35, 40]),
    "M5": (8.5, 5, 4, 2.5, [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50]),
    "M6": (10, 6, 5, 3, [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60]),
    "M8": (13, 8, 6, 4, [10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80]),
    "M10": (16, 10, 8, 5, [16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100]),
    "M12": (18, 12, 10, 6, [20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100, 110, 120]),
}

# ISO 4017 hexagon head screws, thread to the head: width across flats s,
# head height k; preferred lengths.
ISO4017 = {
    "M3": (5.5, 2, [6, 8, 10, 12, 16, 20, 25, 30]),
    "M4": (7, 2.8, [8, 10, 12, 16, 20, 25, 30, 35, 40]),
    "M5": (8, 3.5, [10, 12, 16, 20, 25, 30, 35, 40, 45, 50]),
    "M6": (10, 4, [10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60]),
    "M8": (13, 5.3, [12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80]),
    "M10": (16, 6.4, [16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100]),
    "M12": (18, 7.5, [20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100, 110, 120]),
}

# ISO 10642 hexagon socket countersunk head screws (90 degrees): head
# diameter dk (theoretical), head height k, socket size s, socket depth
# t; preferred lengths (including the head).
ISO10642 = {
    "M3": (6.72, 1.86, 2, 1.1, [8, 10, 12, 16, 20, 25, 30]),
    "M4": (8.96, 2.48, 2.5, 1.5, [8, 10, 12, 16, 20, 25, 30, 35, 40]),
    "M5": (11.2, 3.1, 3, 1.9, [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50]),
    "M6": (13.44, 3.72, 4, 2.2, [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60]),
    "M8": (17.92, 4.96, 5, 3, [10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80]),
    "M10": (22.4, 6.2, 6, 3.6, [12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100]),
    "M12": (26.88, 7.44, 8, 4.3, [20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100]),
}

# ISO 4032 hexagon nuts, style 1: width across flats s, height m.
ISO4032 = {
    "M3": (5.5, 2.4),
    "M4": (7, 3.2),
    "M5": (8, 4.7),
    "M6": (10, 5.2),
    "M8": (13, 6.8),
    "M10": (16, 8.4),
    "M12": (18, 10.8),
}

# ISO 7089 plain washers, normal series: hole d1, outside d2, thickness h.
ISO7089 = {
    "M3": (3.2, 7, 0.5),
    "M4": (4.3, 9, 0.8),
    "M5": (5.3, 10, 1),
    "M6": (6.4, 12, 1.6),
    "M8": (8.4, 16, 1.6),
    "M10": (10.5, 20, 2),
    "M12": (13, 24, 2.5),
}

HEXAGON = "r{c1[c6,c2],c2[c1,c3],c3[c2,c4],c4[c3,c5],c5[c4,c6],c6[c5,c1]}"


def mm(value):
    """An expression in millimetres: `8.5 mm`."""
    text = f"{value:.4f}".rstrip("0").rstrip(".")
    return f"{text} mm"


def parameters(values, comments):
    return [
        {"cmd": "add_parameter", "name": name, "expression": mm(value), "comment": comments.get(name, "")}
        for name, value in values.items()
    ]


def hexagon(sketch, across):
    """Commands for a hexagon centred on a new sketch's origin with its
    width across flats bound to parameter `across`."""
    return [
        {"cmd": "sketch.polygon", "sketch": sketch, "center": [0, 0], "vertex": [1, 0], "sides": 6,
         "inscribed": False},
        {"cmd": "sketch.set_fixed", "sketch": sketch, "entities": ["p13"]},
        {"cmd": "sketch.add_constraint", "sketch": sketch, "constraint": {"type": "vertical", "line": "c1"}},
        {"cmd": "sketch.add_dimension", "sketch": sketch, "dimension": {"type": "diameter", "curve": "c14"},
         "value": across},
    ]


def cylinder(name, diameter, height, operation):
    return {"cmd": "add_feature", "name": name, "def": {
        "type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": diameter, "height": height,
        "operation": operation}}


def extrude(name, sketch, distance, operation, flip=False):
    return {"cmd": "add_feature", "name": name, "def": {
        "type": "extrude", "profiles": [{"sketch": sketch, "region": HEXAGON}],
        "extent": {"type": "distance", "distance": distance}, "flip": flip, "operation": operation}}


COMMENTS = {
    "d": "Nominal thread diameter",
    "P": "Thread pitch",
    "dk": "Head diameter",
    "k": "Head height",
    "s": "Width across flats",
    "t": "Socket depth",
    "L": "Nominal length",
    "mn": "Nut height (m in ISO 4032)",
    "d1": "Hole diameter",
    "d2": "Outside diameter",
    "h": "Thickness",
}


def socket_head(first):
    """ISO 4762: head on the XY plane, shank down, socket from the top."""
    return parameters(first, COMMENTS) + [
        cylinder("Head", "dk", "k", "new_body"),
        cylinder("Shank", "d", "-L", "join"),
        {"cmd": "add_feature", "name": "Head top",
         "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": "k"}}},
        {"cmd": "sketch.create", "plane": "F3", "name": "Socket sketch"},
    ] + hexagon("F4", "s") + [extrude("Socket", "F4", "t", "cut", flip=True)]


def hex_head(first):
    """ISO 4017: hexagon head on the XY plane, shank down."""
    return parameters(first, COMMENTS) + [
        {"cmd": "sketch.create", "plane": "xy", "name": "Head sketch"},
    ] + hexagon("F1", "s") + [
        extrude("Head", "F1", "k", "new_body"),
        cylinder("Shank", "d", "-L", "join"),
    ]


def countersunk(first):
    """ISO 10642: the head's top on the XY plane, a cone down to the shank,
    the socket from the top."""
    return parameters(first, COMMENTS) + [
        {"cmd": "sketch.create", "plane": "xy", "name": "Head top"},
        {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": "dk"},
        {"cmd": "add_feature", "name": "Head bottom plane",
         "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": "-k"}}},
        {"cmd": "sketch.create", "plane": "F2", "name": "Head bottom"},
        {"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": "d"},
        {"cmd": "add_feature", "name": "Head", "def": {
            "type": "loft", "sections": [{"type": "profile", "sketch": "F1", "region": "r{c1}"},
                                         {"type": "profile", "sketch": "F3", "region": "r{c1}"}],
            "ruled": True, "operation": "new_body"}},
        cylinder("Shank", "d", "-L", "join"),
        {"cmd": "sketch.create", "plane": "xy", "name": "Socket sketch"},
    ] + hexagon("F6", "s") + [extrude("Socket", "F6", "t", "cut", flip=True)]


def nut(first):
    """ISO 4032: on the XY plane, the hole through."""
    return parameters(first, COMMENTS) + [
        {"cmd": "sketch.create", "plane": "xy", "name": "Nut sketch"},
    ] + hexagon("F1", "s") + [
        extrude("Nut", "F1", "mn", "new_body"),
        cylinder("Hole", "d", "mn", "cut"),
    ]


def washer(first):
    """ISO 7089: on the XY plane."""
    return parameters(first, COMMENTS) + [
        cylinder("Washer", "d2", "h", "new_body"),
        cylinder("Hole", "d1", "h", "cut"),
    ]


def screw_rows(table, sizes, values, default_length):
    rows = []
    for size in sizes:
        entry = table[size]
        for length in entry[-1]:
            row_values = values(size, entry)
            row_values["L"] = length
            rows.append({"name": f"{size}x{length}", "select": {"Size": size, "Length": str(length)},
                         "values": {name: mm(v) for name, v in row_values.items()}})
    default = next((r["name"] for r in rows if r["select"] == {"Size": default_length[0],
                                                               "Length": str(default_length[1])}),
                   rows[0]["name"])
    return rows, default


def size_rows(table, sizes, values):
    rows = [{"name": size, "select": {"Size": size},
             "values": {name: mm(v) for name, v in values(size, table[size]).items()}} for size in sizes]
    return rows, ("M5" if "M5" in sizes else rows[len(rows) // 2]["name"])


def standards(sizes):
    """Each standard: id, category, path, name, standard, keywords,
    designation, design commands, selectors, parameters, rows, default
    row, and a profile for the preview."""
    out = []

    def screw(sid, table, name, keywords, values, build, folder, profile):
        sizes_here = [s for s in sizes if s in table]
        default_size = "M5" if "M5" in sizes_here else sizes_here[0]
        lengths = table[default_size][-1]
        default_length = (default_size, lengths[min(3, len(lengths) - 1)])
        rows, default = screw_rows(table, sizes_here, values, default_length)
        first = next(r for r in rows if r["name"] == default)
        out.append({
            "id": sid, "category": "screws", "path": f"{folder}/{sid}.mitcad", "name": name,
            "standard": "ISO " + sid[3:], "keywords": keywords,
            "designation": "ISO " + sid[3:] + " {Size}x{Length}",
            "build": build, "selectors": ["Size", "Length"], "rows": rows, "default": default,
            "first": {k: float(v.split()[0]) for k, v in first["values"].items()}, "profile": profile,
        })

    screw("iso4762", ISO4762, "Hexagon socket head cap screw",
          ["socket head", "cap screw", "SHCS", "DIN 912", "allen"],
          lambda size, e: {"d": DIAMETER[size], "P": PITCH[size], "dk": e[0], "k": e[1], "s": e[2], "t": e[3]},
          socket_head, "screws", "socket")
    screw("iso4017", ISO4017, "Hexagon head screw, thread to the head",
          ["hex head", "hex bolt", "set screw", "DIN 933"],
          lambda size, e: {"d": DIAMETER[size], "P": PITCH[size], "s": e[0], "k": e[1]},
          hex_head, "screws", "hex")
    screw("iso10642", ISO10642, "Hexagon socket countersunk head screw",
          ["countersunk", "flat head", "DIN 7991", "socket"],
          lambda size, e: {"d": DIAMETER[size], "P": PITCH[size], "dk": e[0], "k": e[1], "s": e[2], "t": e[3]},
          countersunk, "screws", "countersunk")
    for sid, table, name, keywords, values, build, category, folder, profile in [
        ("iso4032", ISO4032, "Hexagon nut", ["hex nut", "DIN 934", "nut"],
         lambda size, e: {"d": DIAMETER[size], "P": PITCH[size], "s": e[0], "mn": e[1]}, nut, "nuts", "nuts",
         "nut"),
        ("iso7089", ISO7089, "Plain washer, normal series", ["washer", "flat washer", "DIN 125"],
         lambda size, e: {"d1": e[0], "d2": e[1], "h": e[2]}, washer, "washers", "washers", "washer"),
    ]:
        sizes_here = [s for s in sizes if s in table]
        rows, default = size_rows(table, sizes_here, values)
        first = next(r for r in rows if r["name"] == default)
        out.append({
            "id": sid, "category": category, "path": f"{folder}/{sid}.mitcad", "name": name,
            "standard": "ISO " + sid[3:], "keywords": keywords, "designation": "ISO " + sid[3:] + " {Size}",
            "build": build, "selectors": ["Size"], "rows": rows, "default": default,
            "first": {k: float(v.split()[0]) for k, v in first["values"].items()}, "profile": profile,
        })
    return out


# Preview images: the part's outline from the side, drawn from its
# dimensions (no rendering needed), 256 x 256 pixels.

def png(width, height, pixels):
    raw = b"".join(b"\x00" + bytes(row) for row in pixels)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b""))


def preview(item):
    """The side outline of the default size as polygons (x across, z up)."""
    v = item["first"]
    kind = item["profile"]
    shapes = []  # (polygon, shade)
    if kind in ("socket", "hex"):
        head = v["dk"] if kind == "socket" else v["s"] * 2 / math.sqrt(3)
        shapes.append(([(-head / 2, 0), (head / 2, 0), (head / 2, v["k"]), (-head / 2, v["k"])], 0.75))
        shapes.append(([(-v["d"] / 2, -v["L"]), (v["d"] / 2, -v["L"]), (v["d"] / 2, 0), (-v["d"] / 2, 0)], 0.6))
    elif kind == "countersunk":
        shapes.append(([(-v["dk"] / 2, 0), (v["dk"] / 2, 0), (v["d"] / 2, -v["k"]), (-v["d"] / 2, -v["k"])], 0.75))
        shapes.append(([(-v["d"] / 2, -v["L"]), (v["d"] / 2, -v["L"]), (v["d"] / 2, -v["k"]),
                        (-v["d"] / 2, -v["k"])], 0.6))
    elif kind == "nut":
        across = v["s"] * 2 / math.sqrt(3)
        shapes.append(([(-across / 2, 0), (across / 2, 0), (across / 2, v["mn"]), (-across / 2, v["mn"])], 0.75))
        shapes.append(([(-v["d"] / 2, 0), (v["d"] / 2, 0), (v["d"] / 2, v["mn"]), (-v["d"] / 2, v["mn"])], 0.45))
    else:
        shapes.append(([(-v["d2"] / 2, 0), (v["d2"] / 2, 0), (v["d2"] / 2, v["h"] * 3),
                        (-v["d2"] / 2, v["h"] * 3)], 0.75))
        shapes.append(([(-v["d1"] / 2, 0), (v["d1"] / 2, 0), (v["d1"] / 2, v["h"] * 3),
                        (-v["d1"] / 2, v["h"] * 3)], 0.45))
    xs = [x for poly, _ in shapes for x, _ in poly]
    zs = [z for poly, _ in shapes for _, z in poly]
    size = 256
    span = max(max(xs) - min(xs), max(zs) - min(zs))
    scale = (size - 48) / span
    cx, cz = (max(xs) + min(xs)) / 2, (max(zs) + min(zs)) / 2

    def inside(poly, x, z):
        hit = False
        for i in range(len(poly)):
            (x1, z1), (x2, z2) = poly[i], poly[i - 1]
            if (z1 > z) != (z2 > z) and x < (x2 - x1) * (z - z1) / (z2 - z1) + x1:
                hit = not hit
        return hit

    background = (246, 247, 249)
    index = [[-1] * size for _ in range(size)]
    pixels = []
    for row in range(size):
        line = []
        for column in range(size):
            x = (column - size / 2) / scale + cx
            z = (size / 2 - row) / scale + cz
            colour = background
            for number, (poly, shade) in enumerate(shapes):
                if inside(poly, x, z):
                    # Steel, shaded across the part as a turned surface,
                    # lit from the upper left.
                    left = min(px for px, _ in poly)
                    right = max(px for px, _ in poly)
                    u = (x - left) / max(right - left, 1e-9)
                    light = 0.35 + 0.65 * math.sin(math.pi * min(1.0, max(0.0, u * 0.9 + 0.05))) ** 0.6
                    level = max(0.0, min(1.0, shade * light))
                    colour = (int(40 + 175 * level), int(46 + 178 * level), int(56 + 182 * level))
                    index[row][column] = number
            line.extend(colour)
        pixels.append(line)
    # Dark outlines where one part meets another or the background.
    for row in range(1, size - 1):
        for column in range(1, size - 1):
            here = index[row][column]
            if here < 0:
                continue
            around = (index[row - 1][column], index[row + 1][column], index[row][column - 1], index[row][column + 1])
            if any(other != here for other in around):
                pixels[row][3 * column:3 * column + 3] = [38, 42, 50]
    return png(size, size, pixels)


def run(command, cwd=None):
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"{' '.join(map(str, command))} failed:\n{result.stdout}\n{result.stderr}")
    return result.stdout


def write(path, text):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


SOURCES = """# Sources of the dimensions

The dimensions are those of these standards, tabulated for this library
(the values are the standards' facts; no other collection's tables were
copied):

- ISO 4762, hexagon socket head cap screws: head diameter and height
  (maximum), socket size (nominal), socket depth (minimum), preferred
  lengths.
- ISO 4017, hexagon head screws with the thread up to the head: width
  across flats and head height (nominal), preferred lengths.
- ISO 10642, hexagon socket countersunk head screws: theoretical head
  diameter, head height (maximum), socket size, socket depth (minimum),
  preferred lengths (including the head).
- ISO 4032, hexagon regular nuts (style 1): width across flats, height
  (maximum).
- ISO 7089, plain washers, normal series: hole diameter (minimum), outside
  diameter (maximum), thickness (nominal).
- ISO 261 and ISO 724: the coarse thread pitches (parameter P).

The geometry is simplified for assemblies: no threads, chamfers, fillets
or under-head radii. Check critical dimensions against the standard you
work to.

The generator: tools/libraries/make-fastener-library.py in Mitcad's
repository.
"""


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", required=True, help="mitcad-cli")
    parser.add_argument("--out", required=True, help="the library's folder (made when missing)")
    parser.add_argument("--sizes", default=",".join(SIZES), help="sizes, e.g. M3,M5 (default all)")
    parser.add_argument("--standards", default="iso4762,iso4017,iso10642,iso4032,iso7089")
    parser.add_argument("--version", default="1.0.0", help="the library's version")
    parser.add_argument("--commit", action="store_true",
                        help="record the folder as a git version and tag it v<version>")
    parser.add_argument("--work", help="folder for intermediate files (default: next to --out)")
    args = parser.parse_args()

    sizes = [s.strip() for s in args.sizes.split(",") if s.strip()]
    unknown = [s for s in sizes if s not in SIZES]
    if unknown:
        sys.exit(f"unknown sizes {unknown}; known: {SIZES}")
    wanted = [s.strip() for s in args.standards.split(",") if s.strip()]
    items = [item for item in standards(sizes) if item["id"] in wanted]
    if not items:
        sys.exit("no standards to make")
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    if not os.path.isfile(os.path.join(out, ".mitcad", "project.json")):
        run([args.cli, "project", "init", out, "--no-history"])
    work = os.path.abspath(args.work or os.path.dirname(out))
    os.makedirs(work, exist_ok=True)
    components = []
    with tempfile.TemporaryDirectory(prefix=".fastener-work-", dir=work) as scratch:
        for item in items:
            commands = item["build"](item["first"])
            parameters_used = list(item["first"].keys())
            commands.append({"cmd": "set_configurations", "configurations": {
                "selectors": item["selectors"], "parameters": parameters_used, "default": item["default"],
                "rows": item["rows"]}})
            commands.append({"expect": {"bodies": 1}})
            script = os.path.join(scratch, item["id"] + ".json")
            write(script, json.dumps(commands, indent=1))
            target = os.path.join(out, item["path"])
            os.makedirs(os.path.dirname(target), exist_ok=True)
            run([args.cli, "run", script, "--save", target])
            preview_path = f"previews/{item['id']}.png"
            os.makedirs(os.path.join(out, "previews"), exist_ok=True)
            with open(os.path.join(out, preview_path), "wb") as f:
                f.write(preview(item))
            components.append({
                "id": item["id"], "path": item["path"], "category": item["category"], "name": item["name"],
                "standard": item["standard"], "keywords": item["keywords"], "preview": preview_path,
                "designation": item["designation"],
                "description": f"{len(item['rows'])} sizes, {item['rows'][0]['name']} to {item['rows'][-1]['name']}.",
            })
    categories = [c for c in [{"id": "screws", "name": "Screws"}, {"id": "nuts", "name": "Nuts"},
                              {"id": "washers", "name": "Washers"}]
                  if any(item["category"] == c["id"] for item in components)]
    manifest = {
        "format": "mitcad-library", "version": 1, "id": LIBRARY_ID,
        "name": "Mitcad metric fasteners",
        "description": f"ISO metric screws, nuts and washers, {sizes[0]} to {sizes[-1]}, simplified for assemblies.",
        "library_version": args.version, "license": LICENSE, "authors": ["Mitcad contributors"],
        "sources": ["ISO 4762", "ISO 4017", "ISO 10642", "ISO 4032", "ISO 7089", "ISO 261", "ISO 724",
                    "Tabulated for this library; see SOURCES.md"],
        "units": "mm", "categories": categories, "components": components,
    }
    write(os.path.join(out, "mitcad-library.json"), json.dumps(manifest, indent=2) + "\n")
    write(os.path.join(out, "LICENSE"),
          f"The designs, tables and images of this library are dedicated to the public domain\n"
          f"under {LICENSE} (Creative Commons Zero v1.0 Universal).\n"
          f"The licence's text: https://creativecommons.org/publicdomain/zero/1.0/legalcode\n")
    write(os.path.join(out, "SOURCES.md"), SOURCES)
    write(os.path.join(out, "README.md"),
          "# Mitcad metric fasteners\n\n"
          "ISO metric screws, nuts and washers for Mitcad's assemblies: each standard is one design\n"
          "with a table of its sizes. In Mitcad: Libraries, add this repository's URL, Fetch, then\n"
          "Insert Component from Library. Designs record the version they use.\n\n"
          f"Licence: {LICENSE} (LICENSE). Sources of the dimensions: SOURCES.md.\n")
    if args.commit:
        identity = ["-c", "user.name=Mitcad library generator", "-c", "user.email=libraries@mitcad.invalid"]
        if not os.path.isdir(os.path.join(out, ".git")):
            run(["git", "init", "-q", "-b", "main", out])
        run(["git", "-C", out, "add", "-A"])
        status = run(["git", "-C", out, "status", "--porcelain"])
        if status.strip():
            run(["git", "-C", out] + identity + ["commit", "-q", "-m", f"Mitcad metric fasteners {args.version}"])
        run(["git", "-C", out] + identity + ["tag", "-a", "-f", f"v{args.version}", "-m",
                                              f"Mitcad metric fasteners {args.version}"])
    print(f"{LIBRARY_ID} {args.version}: {len(components)} components in {out}")


if __name__ == "__main__":
    main()
