#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Read the actual slicer output, including all nested 3MF placements."""
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
import zipfile

work = Path(sys.argv[1])
ns = {"m": "http://schemas.microsoft.com/3dmanufacturing/core/2015/02"}
identity = [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0]
tolerance = 1e-6


def transform(point, matrix):
    return [sum(point[i] * matrix[3 * i + axis] for i in range(3)) + matrix[9 + axis] for axis in range(3)]


def near(actual, expected):
    assert all(abs(a - b) <= tolerance for a, b in zip(actual, expected)), (actual, expected)


def box(points):
    return ([min(p[i] for p in points) for i in range(3)], [max(p[i] for p in points) for i in range(3)])


def read_case(number):
    receipt = json.loads((work / f"receipts/{number}.json").read_text())
    assert len(receipt["arguments"]) == 1 and receipt["arguments"][0].endswith(".3mf"), receipt
    with zipfile.ZipFile(work / f"receipts/{number}.3mf") as archive:
        root = ET.fromstring(archive.read("3D/3dmodel.model"))
    assert root.attrib.get("unit") == "millimeter", root.attrib
    objects = {obj.attrib["id"]: obj for obj in root.findall("m:resources/m:object", ns)}
    groups = []

    def visit(uid, chain, leaves):
        obj = objects[uid]
        mesh = obj.find("m:mesh", ns)
        if mesh is not None:
            points = [[float(vertex.attrib[a]) for a in "xyz"] for vertex in mesh.findall("m:vertices/m:vertex", ns)]
            assert points and mesh.findall("m:triangles/m:triangle", ns), "empty printed mesh"
            normal_points = [[0, 0, 0], [0, 0, 1]]
            for matrix in chain:
                points = [transform(point, matrix) for point in points]
                normal_points = [transform(point, matrix) for point in normal_points]
            leaves.append({"box": box(points), "normal": [normal_points[1][i] - normal_points[0][i] for i in range(3)]})
        for child in obj.findall("m:components/m:component", ns):
            matrix = [float(value) for value in child.attrib.get("transform", " ".join(map(str, identity))).split()]
            assert len(matrix) == 12, matrix
            visit(child.attrib["objectid"], [matrix] + chain, leaves)

    for item in root.findall("m:build/m:item", ns):
        matrix = [float(value) for value in item.attrib.get("transform", " ".join(map(str, identity))).split()]
        assert len(matrix) == 12, matrix
        leaves = []
        visit(item.attrib["objectid"], [matrix], leaves)
        assert leaves, "empty build object"
        groups.append(leaves)
    return groups


plain = read_case(1)
assert len(plain) == 1 and len(plain[0]) == 1, plain
# Center on Plate and Drop to Bed now default on (mitcad#1109). The
# fixture's default plate is 256 x 256: its centre is exactly (128,128).
# Keep exact world-space bounds as well as all occurrence/normal checks.
near(plain[0][0]["box"][0], [118, 123, 0])
near(plain[0][0]["box"][1], [138, 133, 5])
near(plain[0][0]["normal"], [0, 0, 1])

bed = read_case(2)
assert len(bed) == 2 and all(len(group) == 1 for group in bed), bed
boxes = []
for group in bed:
    leaf = group[0]
    near(leaf["normal"], [0, 0, -1])
    low, high = leaf["box"]
    assert abs(low[2]) <= tolerance and abs(high[2] - 5) <= tolerance, leaf
    boxes.append((low, high))
boxes.sort(key=lambda value: value[0][0])
assert abs(boxes[1][0][0] - boxes[0][1][0] - 5) <= tolerance, boxes
near(boxes[0][0], [105.5, 123, 0])
near(boxes[0][1], [125.5, 133, 5])
near(boxes[1][0], [130.5, 123, 0])
near(boxes[1][1], [150.5, 133, 5])

whole = read_case(3)
assert len(whole) == 1 and len(whole[0]) == 2, whole
leaves = sorted(whole[0], key=lambda value: value["box"][0][0])
near(leaves[0]["box"][0], [93, 125.5, 0])
near(leaves[0]["box"][1], [113, 135.5, 5])
near(leaves[0]["normal"], [0, 0, 1])
near(leaves[1]["box"][0], [143, 120.5, 0])
near(leaves[1]["box"][1], [163, 125.5, 10])
near(leaves[1]["normal"], [0, -1, 0])

chosen = read_case(4)
assert len(chosen) == 1 and len(chosen[0]) == 2, chosen
first, second = sorted(chosen[0], key=lambda value: value["box"][0][0])
near(first["box"][0], [93, 128, 5])
near(first["box"][1], [113, 133, 15])
near(second["box"][0], [143, 123, 0])
near(second["box"][1], [163, 133, 5])
near(first["normal"], [0, -1, 0])
near(second["normal"], [0, 0, -1])
near([second["box"][0][i] - first["box"][0][i] for i in range(3)], [50, -5, -5])
near([first["box"][1][i] - first["box"][0][i] for i in range(3)], [20, 5, 10])
near([second["box"][1][i] - second["box"][0][i] for i in range(3)], [20, 10, 5])
assert abs(second["box"][0][2]) <= tolerance and abs(second["box"][1][2] - 5) <= tolerance, second

for name in ["plain", "bed"]:
    before = json.loads((work / f"{name}-before.mitcad").read_text())
    after = json.loads((work / f"{name}.mitcad").read_text())
    assert before == after, f"printing changed the saved {name} model"
assert sorted(path.name for path in (work / "receipts").glob("*.json")) == [f"{i}.json" for i in range(1, 5)], "unexpected slicer result count"
assert sorted(path.name for path in (work / "receipts").glob("*.started")) == [f"{i}.started" for i in range(1, 5)], "unexpected slicer invocation count"
print("Verified: selected occurrence, independent bed objects, assembled send, chosen bed face, zero-send Escape, original model files")
