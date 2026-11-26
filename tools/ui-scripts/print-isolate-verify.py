# SPDX-License-Identifier: MIT
"""Checks the files Send to Slicer wrote with Isolate (mitcad#2475).

Usage: print-isolate-verify.py stl <folder> | 3mf <file.3mf>

tools/ui-scripts/print-isolate.json places Pin (20 x 10 x 5 mm) twice:
Pin:1 as designed and Pin:2 turned a quarter about z (10 x 20 x 5 as
placed). With Pin:2 isolated and selected, the STL folder holds one file,
Pin:2's, and the 3MF one placement of the pin; both as Pin:2 places it,
10 mm along x and 20 mm along y.
"""
import os
import struct
import sys
import xml.etree.ElementTree as ET
import zipfile

NS = {"m": "http://schemas.microsoft.com/3dmanufacturing/core/2015/02"}
PIN_2 = (10.0, 20.0, 5.0)


def extents(points):
    return tuple(max(p[k] for p in points) - min(p[k] for p in points) for k in range(3))


def check_extents(what, points):
    size = extents(points)
    print(f"     {what}: {size[0]:.3f} x {size[1]:.3f} x {size[2]:.3f}")
    if any(abs(size[k] - PIN_2[k]) > 1e-3 for k in range(3)):
        sys.exit(f"FAIL: {what} is {size}, not Pin:2's {PIN_2}")


def stl(folder):
    names = sorted(n for n in os.listdir(folder) if n.lower().endswith(".stl"))
    if names != ["Pin (Pin_2).stl"]:
        sys.exit(f"FAIL: the STL files are {names}, not Pin:2's alone")
    data = open(os.path.join(folder, names[0]), "rb").read()
    count = struct.unpack_from("<I", data, 80)[0]
    if len(data) != 84 + 50 * count:
        sys.exit("FAIL: the STL file is cut short")
    points = [struct.unpack_from("<3f", data, 84 + 50 * i + 12 + 12 * v) for i in range(count) for v in range(3)]
    check_extents(names[0], points)


def matrix(element):
    text = element.get("transform") or "1 0 0 0 1 0 0 0 1 0 0 0"
    return [float(v) for v in text.split()]


def compose(inner, outer):
    # 3MF maps row vectors, p' = p * M with the move last: inner first.
    m = [0.0] * 12
    for r in range(3):
        for c in range(3):
            m[3 * r + c] = sum(inner[3 * r + k] * outer[3 * k + c] for k in range(3))
    for c in range(3):
        m[9 + c] = sum(inner[9 + k] * outer[3 * k + c] for k in range(3)) + outer[9 + c]
    return m


def apply(m, p):
    return [p[0] * m[c] + p[1] * m[3 + c] + p[2] * m[6 + c] + m[9 + c] for c in range(3)]


def three_mf(path):
    model = ET.fromstring(zipfile.ZipFile(path).read("3D/3dmodel.model"))
    objects = {o.get("id"): o for o in model.findall("m:resources/m:object", NS)}
    items = model.findall("m:build/m:item", NS)
    if len(items) != 1:
        sys.exit(f"FAIL: {len(items)} build items, not one")
    # Every mesh the item places, each with its whole transform.
    placed = []

    def walk(object_id, transform):
        obj = objects[object_id]
        mesh = obj.find("m:mesh", NS)
        if mesh is not None:
            placed.append((mesh, transform))
        for component in obj.findall("m:components/m:component", NS):
            walk(component.get("objectid"), compose(matrix(component), transform))

    walk(items[0].get("objectid"), matrix(items[0]))
    if len(placed) != 1:
        sys.exit(f"FAIL: {len(placed)} placements in the 3MF, not Pin:2's alone")
    mesh, transform = placed[0]
    points = [apply(transform, [float(v.get(a)) for a in "xyz"]) for v in mesh.findall("m:vertices/m:vertex", NS)]
    check_extents("the 3MF's placement", points)


if __name__ == "__main__":
    if len(sys.argv) != 3 or sys.argv[1] not in ("stl", "3mf"):
        sys.exit(__doc__)
    (stl if sys.argv[1] == "stl" else three_mf)(sys.argv[2])
    print(f"ok   {sys.argv[1]}: Pin:2 alone")
