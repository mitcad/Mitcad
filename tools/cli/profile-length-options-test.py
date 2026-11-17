# SPDX-License-Identifier: MIT
"""The length option of the parts cut to length (mitcad#1950), read from
tools/libraries/make-profile-library.py and its group files
(tools/libraries/profiles) without mitcad-cli: every part whose group file
gives `build.length` gets its length parameter (L) as the last option of its configuration
table (length_table), and that parameter's limits are
the part's shortest and longest (length_limits: `build.length`'s min and
max, the shortest above 0 and below the longest). Every stock length has a
row that sets L to it, every row that sets L sets it within the limits,
every `Stock` (or `Length`) row of a group file sets L, the rows set only
options, their names are unique and the default names one of them. A
typed tube keeps its size and wall before L; a panel's widths are an
option limited to its rows' widths by a rule. A closed table of standard
sizes (rows that set several other sizes: a timber board's or a steel
tube's `sizes`) has L as an open option (`open`) beside its rows, which
leave it out.

    python3 profile-length-options-test.py --libraries <tools/libraries>
"""

import argparse
import importlib.util
import os
import sys

FAILURES = []


def check(condition, text):
    if not condition:
        FAILURES.append(text)


def load(libraries):
    path = os.path.join(libraries, "make-profile-library.py")
    spec = importlib.util.spec_from_file_location("make_profile_library", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def static_default(g, entry, length, name):
    """The part's length in its design where the group file gives it (a
    model part's parameter, or build.length.default); None for a section,
    whose length is its source's."""
    if length.get("default") is not None:
        return g.length_of(length["default"], entry["id"])
    parameters = (entry.get("dimensions") or {}).get("parameters") or {}
    if name in parameters:
        return g.length_of(parameters[name], entry["id"])
    return None


def check_entry(g, entry):
    """One part cut to length: its table."""
    what = entry["id"]
    length = entry["build"]["length"]
    name = (length.get("parameter") or "L").strip()
    low, high = g.length_limits(entry, length)
    check(0 < low < high, f"{what}: its shortest {low} and longest {high}")
    dimensions = entry.get("dimensions") or {}
    if entry.get("typed") is not None:
        # Its open table of sizes, as make_model makes it.
        g.typed_tube(entry, dimensions, dict(dimensions.get("parameters") or {}))
    default = static_default(g, entry, length, name)
    if default is not None:
        check(low - 1e-9 <= default <= high + 1e-9,
              f"{what}: its length in the design, {default:g} mm, is not within {low:g} to {high:g} mm")
    table = g.length_table(entry, length, default)
    rows_given = entry.get("rows") or []
    check(table is not None, f"{what}: no configuration table with its length {name}")
    if table is None:
        return None
    if table.get("closed"):
        # A closed table of sizes: L open beside its rows, which leave it out.
        sizes = {key for row in rows_given for key in (row.get("values") or {}) if key != name}
        check(len(sizes) > 1, f"{what}: a closed table, though its rows set {sorted(sizes) or 'no other size'}")
        check(table.get("open") == [name] and table.get("options") == [name],
              f"{what}: its closed table's open options {table.get('open')} are not its length {name}")
        for row in table["rows"]:
            check(name not in (row.get("values") or {}), f"{what}: row {row['name']} sets its open length {name}")
        names = [row["name"] for row in table["rows"]]
        check(len(set(names)) == len(names), f"{what}: rows of the same name")
        check(table.get("default") in names, f"{what}: the default {table.get('default')} is no row")
        return table
    options = table["options"]
    check(not table.get("closed"), f"{what}: its table is closed")
    check(options and options[-1] == name, f"{what}: its options {options} do not end with its length {name}")
    names = [row["name"] for row in table["rows"]]
    check(len(set(names)) == len(names), f"{what}: rows of the same name in {names}")
    if table["rows"]:
        check(table.get("default") in names or "default" not in table,
              f"{what}: the default {table.get('default')} is no row of {names}")
    for row in table["rows"]:
        values = row.get("values") or {}
        check(values, f"{what}: row {row['name']} sets nothing")
        stray = [key for key in values if key not in options]
        check(not stray, f"{what}: row {row['name']} sets {stray}, which are no options")
        if name in values:
            value = g.length_of(values[name], what)
            check(low - 1e-9 <= value <= high + 1e-9,
                  f"{what}: row {row['name']} sets {name} to {value:g} mm, outside {low:g} to {high:g} mm")
    for stock in length.get("stock") or []:
        value = g.length_of(stock, what)
        check(any(abs(g.length_of((row.get("values") or {}).get(name, "0"), what) - value) <= 1e-6
                  for row in table["rows"]),
              f"{what}: no row sets the stock length {value:g} mm")
    for row in rows_given:
        select = row.get("select") or {}
        if "Stock" in select or "Length" in select:
            made = next((r for r in table["rows"] if r["name"] == row["name"]), None)
            check(made is not None and name in (made.get("values") or {}),
                  f"{what}: its stock row {row['name']} does not set {name}")
            if made is not None and row.get("part_number"):
                check(made.get("part_number") == row["part_number"],
                      f"{what}: its stock row {row['name']} lost its part number")
    if entry.get("typed") is not None:
        check(options[:-1] == entry["_table"]["options"],
              f"{what}: a typed tube's options {options}, not its sizes then {name}")
    if default is not None and table["rows"] and not rows_given and entry.get("typed") is None:
        first = next(r for r in table["rows"] if r["name"] == table["default"])
        check(abs(g.length_of(first["values"][name], what) - default) <= 1e-6,
              f"{what}: its default row {first['name']} is not its length in the design, {default:g} mm")
    return table


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--libraries", required=True, help="tools/libraries")
    args = parser.parse_args()
    libraries = os.path.abspath(args.libraries)
    sys.path.insert(0, libraries)
    g = load(libraries)
    entries, _, _ = g.read_groups(os.path.join(libraries, "profiles"), set())
    cut = [e for e in entries if isinstance((e.get("build") or {}).get("length"), dict) and e["build"]["length"]]
    check(len(cut) >= 150, f"only {len(cut)} parts cut to length in the group files")
    tables, closed = {}, []
    for entry in cut:
        table = check_entry(g, entry)
        if table is not None:
            tables[entry["id"]] = table
            if table.get("closed"):
                closed.append(entry["id"])
    check(len(tables) == len(cut), f"{len(cut) - len(tables)} parts cut to length without a length option")
    # Spot checks: a slot profile, a rail's stock rows, a panel's widths, a
    # typed tube.
    profile = tables.get("p40x40-lr-i8")
    check(profile is not None and profile["options"] == ["L"], f"p40x40-lr-i8: options {profile and profile['options']}")
    rail = tables.get("rail-ar-hr15")
    check(rail is not None and {r["name"]: r["values"].get("L") for r in rail["rows"]} ==
          {"600": "600 mm", "960": "960 mm", "1320": "1320 mm", "1980": "1980 mm"},
          f"rail-ar-hr15: rows {rail and rail['rows']}")
    panel = tables.get("panel-alu-4")
    check(panel is not None and panel["options"] == ["W", "L"] and panel["rules"]
          and panel["rules"][0]["then"][0]["option"] == "W", f"panel-alu-4: table {panel}")
    tube = tables.get("shs-typed")
    check(tube is not None and tube["options"] == ["B", "T", "L"], f"shs-typed: options {tube and tube['options']}")
    board = tables.get("timber-sawn")
    check(board is not None and board.get("closed") and board.get("open") == ["L"],
          f"timber-sawn: table {board and {k: board[k] for k in ('options', 'open', 'closed')}}")
    print(f"profile length options: {len(tables)} of {len(cut)} parts cut to length have the length option, "
          f"{len(closed)} of them beside a closed table of sizes")
    if FAILURES:
        sys.exit("profile length options: FAILED:\n" + "\n".join(FAILURES))
    print("profile length options: ok")


if __name__ == "__main__":
    main()
