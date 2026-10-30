# SPDX-License-Identifier: MIT
"""The fastener generator's dimension tables (mitcad#900), read from
tools/libraries/make-fastener-library.py without mitcad-cli: every size of
a table is one of SIZES (a size spelt otherwise would be left out without
a word), every size has its diameter and the ISO 261 coarse pitch, the
lengths are ascending in the standards' length series, the dimensions
make a part (a head wider than the thread, a socket inside the head, a
countersunk head of 90 degrees, a washer's hole over the thread), each
standard's rows and default row, and every row of the library's earlier
versions is still there (designs may use them). Since mitcad#949 each
component stands for its ISO and DIN standard: every row names its
standards (the DIN-only lengths, the DIN head's own sizes, the suppliers'
lengths in no table as non-standard), the rows' combinations are unique,
and the suppliers' articles (fastener_suppliers.py) name a known supplier,
a date and a part number in the supplier's form.

    python3 fastener-tables-test.py --libraries <tools/libraries>
"""

import argparse
import importlib.util
import math
import os
import re
import sys

# ISO 261 coarse pitches.
COARSE_PITCH = {1.6: 0.35, 2: 0.4, 2.5: 0.45, 3: 0.5, 4: 0.7, 5: 0.8, 6: 1, 8: 1.25, 10: 1.5, 12: 1.75, 14: 2,
                16: 2, 20: 2.5, 24: 3}

# The nominal lengths of the ISO screw standards' length series.
LENGTH_SERIES = {2, 2.5, 3, 4, 5, 6, 8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100, 110,
                 120, 130, 140, 150, 160, 180, 200, 220, 240}
# The DIN standards' lengths also have these.
DIN_LENGTHS = LENGTH_SERIES | {7, 14, 18, 22, 28, 75, 170, 190, 260, 280, 300}

# Each component: its rows, first and last row, default row.
EXPECTED = {
    "iso4762": (448, "M1.6x2.5", "M24x500", "M5x16"),
    "iso4017": (429, "M1.6x2", "M24x300", "M5x20"),
    "iso10642": (121, "M3x8", "M20x100", "M5x16"),
    "iso4032": (23, "M1.6", "M24 DIN 934", "M5"),
    "iso7089": (14, "M1.6", "M24", "M5"),
}

# Each component's standards (mitcad#949), in the order of its `standard`.
STANDARDS = {
    "iso4762": ["ISO 4762", "DIN 912"],
    "iso4017": ["ISO 4017", "DIN 933"],
    "iso10642": ["ISO 10642", "DIN 7991"],
    "iso4032": ["ISO 4032", "DIN 934"],
    "iso7089": ["ISO 7089", "DIN 125"],
}

# Rows checked one by one: component, row, its option values, standards,
# own designation (None: the pattern's), and values ({parameter: mm}).
ROWS = [
    ("iso4762", "M6x8", {"Size": "M6", "Length": "8"}, ["DIN 912"], None, {}),
    ("iso4762", "M6x6", {"Size": "M6", "Length": "6"}, ["non-standard"], None, {}),
    ("iso4762", "M6x10", {"Size": "M6", "Length": "10"}, ["ISO 4762", "DIN 912"], None, {}),
    ("iso4762", "M8x10", {"Size": "M8", "Length": "10"}, ["DIN 912"], None, {}),
    ("iso4762", "M24x240", {"Size": "M24", "Length": "240"}, ["ISO 4762", "DIN 912"], None, {}),
    ("iso4762", "M12x300", {"Size": "M12", "Length": "300"}, ["DIN 912"], None, {}),
    ("iso4017", "M6x10", {"Size": "M6", "Length": "10"}, ["DIN 933"], None, {"s": 10}),
    ("iso4017", "M10x20", {"Size": "M10", "Length": "20"}, ["ISO 4017"], None, {"s": 16, "k": 6.4}),
    ("iso4017", "M10x20 DIN 933", {"Size": "M10 DIN 933", "Length": "20"}, ["DIN 933"], "DIN 933 M10x20",
     {"s": 17, "k": 6.4}),
    ("iso4017", "M14x10 DIN 933", {"Size": "M14 DIN 933", "Length": "10"}, ["DIN 933"], "DIN 933 M14x10",
     {"s": 22}),
    ("iso4017", "M10x16", {"Size": "M10", "Length": "16"}, ["non-standard"], None, {"s": 16}),
    ("iso4017", "M12x20", {"Size": "M12", "Length": "20"}, ["non-standard"], None, {"s": 18}),
    ("iso4017", "M12x220 DIN 933", {"Size": "M12 DIN 933", "Length": "220"}, ["non-standard"],
     "M12x220 (non-standard)", {"s": 19}),
    ("iso4017", "M20x200", {"Size": "M20", "Length": "200"}, ["ISO 4017", "DIN 933"], None, {}),
    ("iso4017", "M24x200", {"Size": "M24", "Length": "200"}, ["ISO 4017", "DIN 933"], None, {}),
    ("iso4017", "M5x7", {"Size": "M5", "Length": "7"}, ["DIN 933"], None, {}),
    ("iso10642", "M14x30", {"Size": "M14", "Length": "30"}, ["ISO 10642", "DIN 7991"], None, {"dk": 30.8, "k": 8.4}),
    ("iso10642", "M14x65", {"Size": "M14", "Length": "65"}, ["ISO 10642"], None, {}),
    ("iso4032", "M4", {"Size": "M4"}, ["ISO 4032", "DIN 934"], None, {"s": 7, "mn": 3.2}),
    ("iso4032", "M10", {"Size": "M10"}, ["ISO 4032"], None, {"s": 16, "mn": 8.4}),
    ("iso4032", "M10 DIN 934", {"Size": "M10 DIN 934"}, ["DIN 934"], "DIN 934 M10", {"s": 17, "mn": 8}),
    ("iso7089", "M8", {"Size": "M8"}, ["ISO 7089", "DIN 125"], None, {"d1": 8.4, "d2": 16, "h": 1.6}),
]

# A sample of articles, as read from the catalogues: component, row,
# supplier, part number, material, finish.
ARTICLES = [
    ("iso4762", "M6x8", "wurth", "0084 6 8", "8.8", "zinc-plated"),
    ("iso4762", "M6x6", "wurth", "0084 6 6", "8.8", "zinc-plated"),
    ("iso4762", "M10x20", "etra", "701200210020", "8.8", "zinc-plated"),
    ("iso4017", "M10x20 DIN 933", "wurth", "0057 10 20", "8.8", "zinc-plated"),
    ("iso4017", "M10x20", "wurth", "0057 910 20", "8.8", "zinc-plated"),
    ("iso10642", "M6x10", "etra", "701258706010", "10.9", "zinc-plated"),
    ("iso4032", "M10 DIN 934", "wurth", "0317 10", "8", "zinc-plated"),
    ("iso4032", "M10", "wurth", "0317 910", "8", "zinc-plated"),
    ("iso7089", "M8", "etra", "702105908", "steel 200 HV", "zinc-plated"),
]

SUPPLIER_FORM = {"wurth": re.compile(r"\d{4}( [0-9]{1,3}){1,2}"), "etra": re.compile(r"\d{9,15}")}

# The rows of the library up to mitcad-parts 2.7.0 (M3 to M12), kept.
EARLIER_LENGTHS = {
    "iso4762": {
        "M3": [5, 6, 8, 10, 12, 16, 20, 25, 30],
        "M4": [6, 8, 10, 12, 16, 20, 25, 30, 35, 40],
        "M5": [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50],
        "M6": [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60],
        "M8": [10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80],
        "M10": [16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100],
        "M12": [20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100, 110, 120],
    },
    "iso4017": {
        "M3": [6, 8, 10, 12, 16, 20, 25, 30],
        "M4": [8, 10, 12, 16, 20, 25, 30, 35, 40],
        "M5": [10, 12, 16, 20, 25, 30, 35, 40, 45, 50],
        "M6": [10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60],
        "M8": [12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80],
        "M10": [16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100],
        "M12": [20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100, 110, 120],
    },
    "iso10642": {
        "M3": [8, 10, 12, 16, 20, 25, 30],
        "M4": [8, 10, 12, 16, 20, 25, 30, 35, 40],
        "M5": [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50],
        "M6": [8, 10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60],
        "M8": [10, 12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80],
        "M10": [12, 16, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100],
        "M12": [20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 80, 90, 100],
    },
}
EARLIER_SIZES = ["M3", "M4", "M5", "M6", "M8", "M10", "M12"]

FAILURES = []


def check(condition, message):
    if not condition:
        FAILURES.append(message)


def load(libraries):
    path = os.path.join(libraries, "make-fastener-library.py")
    spec = importlib.util.spec_from_file_location("make_fastener_library", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def check_tables(g):
    tables = {"iso4762": g.ISO4762, "iso4017": g.ISO4017, "iso10642": g.ISO10642, "iso4032": g.ISO4032,
              "iso7089": g.ISO7089}
    for size in g.SIZES:
        check(size in g.DIAMETER, f"{size}: no diameter")
        check(size in g.PITCH, f"{size}: no pitch")
        if size in g.DIAMETER:
            check(size == f"M{g.DIAMETER[size]:g}", f"{size}: diameter {g.DIAMETER[size]}")
            check(g.PITCH.get(size) == COARSE_PITCH.get(g.DIAMETER[size]),
                  f"{size}: pitch {g.PITCH.get(size)}, ISO 261 {COARSE_PITCH.get(g.DIAMETER[size])}")
    for sid, table in tables.items():
        for size in table:
            check(size in g.SIZES, f"{sid}: {size} is not one of SIZES {g.SIZES}")
    for sid in ("iso4762", "iso4017", "iso10642"):
        for size, entry in tables[sid].items():
            lengths = entry[-1]
            check(lengths == sorted(set(lengths)), f"{sid} {size}: lengths not ascending {lengths}")
            check(set(lengths) <= LENGTH_SERIES, f"{sid} {size}: lengths outside the series "
                                                 f"{sorted(set(lengths) - LENGTH_SERIES)}")
    for size, (dk, k, s, t, lengths) in g.ISO4762.items():
        d = g.DIAMETER.get(size, 0)
        check(dk > d and s < d and t < k and s * 2 / math.sqrt(3) < dk,
              f"iso4762 {size}: dk {dk}, k {k}, s {s}, t {t} make no socket head")
    for size, (s, k, lengths) in g.ISO4017.items():
        d = g.DIAMETER.get(size, 0)
        check(s > d and 0 < k < d, f"iso4017 {size}: s {s}, k {k} make no hexagon head")
    for size, (dk, k, s, t, lengths) in g.ISO10642.items():
        d = g.DIAMETER.get(size, 0)
        # A 90-degree head: its height half the difference of the diameters.
        check(abs(k - (dk - d) / 2) < 0.01, f"iso10642 {size}: k {k} is not (dk - d) / 2 = {(dk - d) / 2:g}")
        check(s < d and t < k and lengths[0] > k, f"iso10642 {size}: s {s}, t {t}, L {lengths[0]}")
    for size, (s, m) in g.ISO4032.items():
        d = g.DIAMETER.get(size, 0)
        check(s > d and 0.7 * d < m < d, f"iso4032 {size}: s {s}, m {m}")
    for size, (d1, d2, h) in g.ISO7089.items():
        d = g.DIAMETER.get(size, 0)
        check(d < d1 < d2 and 0 < h < d1, f"iso7089 {size}: d1 {d1}, d2 {d2}, h {h}")
    for name, lengths_of, iso in (("DIN 912", g.DIN912_LENGTHS, g.ISO4762), ("DIN 933", g.DIN933_LENGTHS, g.ISO4017),
                                  ("DIN 7991", g.DIN7991_LENGTHS, g.ISO10642)):
        for size, lengths in lengths_of.items():
            check(size in iso, f"{name} {size}: not a size of the ISO table")
            check(lengths == sorted(set(lengths)), f"{name} {size}: lengths not ascending {lengths}")
            check(set(lengths) <= DIN_LENGTHS, f"{name} {size}: lengths outside the series "
                                               f"{sorted(set(lengths) - DIN_LENGTHS)}")
    for size, s in g.DIN933_HEAD.items():
        check(s != g.ISO4017[size][0], f"DIN 933 {size}: s {s} is ISO 4017's")
    for size, (s, m) in g.DIN934.items():
        check(size in g.ISO4032 and (s, m) != g.ISO4032[size], f"DIN 934 {size}: s {s}, m {m} are ISO 4032's")
        d = g.DIAMETER.get(size, 0)
        check(s > d and 0.6 * d < m < d, f"DIN 934 {size}: s {s}, m {m}")


def check_standards(g, items):
    """Each row's standards, combination, designation and values
    (mitcad#949)."""
    for item in items.values():
        sid = item["id"]
        names = STANDARDS[sid]
        check(item["standard"] == " / ".join(names), f"{sid}: standard {item['standard']}")
        pattern = "{standard} {Size}x{Length}" if item["selectors"] == ["Size", "Length"] else "{standard} {Size}"
        check(item["designation"] == pattern, f"{sid}: designation {item['designation']}")
        combinations = set()
        for row in item["rows"]:
            tags = row.get("standards") or []
            ordered = bool(tags) and tags == [t for t in names if t in tags]
            check(ordered or tags == ["non-standard"], f"{sid} {row['name']}: standards {tags}")
            if sid not in ("iso4762", "iso4017"):
                check(tags != ["non-standard"], f"{sid} {row['name']}: a non-standard row")
            combination = tuple(sorted(row["select"].items()))
            check(combination not in combinations, f"{sid} {row['name']}: another row has {row['select']}")
            combinations.add(combination)
            check(not set(row) - {"name", "select", "values", "designation", "standards", "supplier_parts"},
                  f"{sid} {row['name']}: fields {sorted(row)}")
    for sid, name, select, tags, designation, values in ROWS:
        row = next((r for r in items[sid]["rows"] if r["name"] == name), None)
        if row is None:
            check(False, f"{sid}: no row {name}")
            continue
        check(row["select"] == select, f"{sid} {name}: options {row['select']}, not {select}")
        check(row["standards"] == tags, f"{sid} {name}: standards {row['standards']}, not {tags}")
        check(row.get("designation") == designation, f"{sid} {name}: designation {row.get('designation')}")
        for parameter, value in values.items():
            got = float(row["values"][parameter].split()[0])
            check(abs(got - value) < 1e-6, f"{sid} {name}: {parameter} {got}, not {value}")


def check_articles(g, s, items):
    """The suppliers' articles: a known supplier, the date, a part number in
    the supplier's form, a material and a finish; only the countersunk
    screws' lengths in no table are left without a row."""
    suppliers = {sup["id"] for sup in g.SUPPLIERS}
    check(suppliers == {"wurth", "etra"} and all(sup["url"].startswith("https://") for sup in g.SUPPLIERS),
          f"suppliers {g.SUPPLIERS}")
    for item in items.values():
        counts = {}
        for row in item["rows"]:
            for part in row.get("supplier_parts", []):
                where = f"{item['id']} {row['name']} {part}"
                check(part["supplier"] in suppliers, f"{where}: unknown supplier")
                check(part["checked"] == s.CHECKED and re.fullmatch(r"\d{4}-\d{2}-\d{2}", part["checked"]),
                      f"{where}: checked")
                check(bool(SUPPLIER_FORM.get(part["supplier"], re.compile("")).fullmatch(part["part_number"])),
                      f"{where}: part number")
                check(part["material"] and part["finish"] in ("zinc-plated", "plain"), f"{where}: material, finish")
                counts[part["supplier"]] = counts.get(part["supplier"], 0) + 1
        print(f"{item['id']}: articles {counts}")
        check(set(counts) == suppliers, f"{item['id']}: articles of {sorted(counts)} only")
    for sid, name, supplier, number, material, finish in ARTICLES:
        row = next((r for r in items[sid]["rows"] if r["name"] == name), {})
        check(any(p["supplier"] == supplier and p["part_number"] == number and p["material"] == material
                  and p["finish"] == finish for p in row.get("supplier_parts", [])),
              f"{sid} {name}: no article {supplier} {number} ({material} {finish})")
    for key in ("00846  8", "008425 6", "0057910", "03176", "464381603", "0097010100"):
        grouped = s.wurth_number(key)
        first, middle, *rest = grouped.split(" ")
        back = first + (middle.ljust(3) + rest[0] if rest else middle)
        check(back == key, f"Würth {key!r}: {grouped!r} reads back as {back!r}")
    left = g.unplaced_articles(list(items.values()))
    check(all(entry[0] == "iso10642" for entry in left),
          f"articles without a row: {[entry for entry in left if entry[0] != 'iso10642'][:5]}")


def check_rows(g):
    items = {item["id"]: item for item in g.standards(g.SIZES)}
    for sid, (count, first, last, default) in EXPECTED.items():
        item = items.get(sid)
        if item is None:
            check(False, f"{sid}: not made")
            continue
        names = [r["name"] for r in item["rows"]]
        print(f"{sid}: {len(names)} rows, {names[0]} to {names[-1]}, default {item['default']}")
        check(len(names) == count, f"{sid}: {len(names)} rows, not {count}")
        check(len(set(names)) == len(names), f"{sid}: rows of the same name")
        check(names[0] == first and names[-1] == last, f"{sid}: rows {names[0]} to {names[-1]}, not {first} to "
                                                       f"{last}")
        check(item["default"] == default, f"{sid}: default row {item['default']}, not {default}")
        if sid in EARLIER_LENGTHS:
            missing = [f"{size}x{length}" for size, lengths in EARLIER_LENGTHS[sid].items() for length in lengths
                       if f"{size}x{length}" not in names]
        else:
            missing = [size for size in EARLIER_SIZES if size not in names]
        check(not missing, f"{sid}: the earlier rows {missing} are gone")
    return items


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--libraries", required=True, help="tools/libraries")
    args = parser.parse_args()
    sys.path.insert(0, os.path.abspath(args.libraries))
    g = load(os.path.abspath(args.libraries))
    check_tables(g)
    items = check_rows(g)
    check_standards(g, items)
    check_articles(g, sys.modules["fastener_suppliers"], items)
    if FAILURES:
        sys.exit("fastener tables: FAILED:\n" + "\n".join(FAILURES))
    print("fastener tables: ok")


if __name__ == "__main__":
    main()
