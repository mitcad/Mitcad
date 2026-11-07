# SPDX-License-Identifier: MIT
"""The index of a rounding-spec run, and the specs run again (mitcad#1398).

    python3 tools/f3d-rounding-specs-index.py OUT
    python3 tools/f3d-rounding-specs-index.py OUT --run TOOL [--class NAME]...
        [--spec ID]... [--jobs N] [--timeout S] [--describe]

OUT is the folder of tools/f3d-rounding-specs.sh. The first form reads
OUT/files.tsv, OUT/done/*, OUT/specs/<content id>/*.jsonl (the import's
dressup dump, core/import-replay/src/dressups.rs) and OUT/reports/*.json
(the import's reports), and writes:

- OUT/index.tsv: one row per spec (a fillet or chamfer definition the
  geometry kernel failed on): its id (`<content id>/<design>/T<item>-<n>`),
  the corpus variable, the content id, the item, its feature name and type,
  the sizes (parameters resolved), the number of edges and faces selected,
  the class of the error and the error itself, the item's outcome in the
  import (a spec of an item that ends parametric failed only in an earlier
  candidate; `linked`: an item of another document the file refers to,
  imported with it, whose outcome is in that document's report), the files (input, before, after; relative to OUT/specs), the
  signature distances to the stored bodies and the history's states before
  and after the item (one state for both: the file stores no state between,
  so the stored body "after" is not the rounding's alone);
- OUT/summary.txt: the files run, and the specs counted by class and by
  outcome (all, and items once).

The second form runs the specs again, each in a process of its own, with
TOOL (mitcad-rounding-specs, tools/rounding-specs) on its input body,
compared with the stored body after it, and writes OUT/results.tsv (the
spec, its class, MATCH / NEAR / OFF / INVALID / FAIL / TIMEOUT / SKIPPED,
the faces and changes of volume, the error; rows of specs not run this
time are kept) and each run's output to OUT/runs/. `--class` and `--spec`
choose the specs (by the index's class, by id); without them every spec
runs. `--describe` has the tool print the selected edges and the faces
round them first.

The rows name designs only by content id and the file's own feature names
(Fillet4, Chamfer2).
"""

import argparse
import collections
import concurrent.futures
import glob
import json
import os
import subprocess
import sys

# The error classes, by the start of the note after "fillet failed; " or
# "chamfer failed; " (geometry/src/dressup.cpp, fillet_failure and
# corner_failure; core/import/docs/f3d-rounds-2.md has the tables). The
# first that matches is taken.
CLASSES = [
    ("walk between two faces", "walk between the two faces fails"),
    ("walk rolling on an edge", "where it rolls on an edge"),
    ("no start", "finds no start for the rounding"),
    ("twisted surface", "twists:"),
    ("computation fails", "computation of the rounding"),
    ("overlap across a face", "overlap on the face between them"),
    ("contacts crossing", "contacts cross"),
    ("contacts crossing", "do not intersect"),
    ("two contours intersect", "intersect each other"),
    ("chamfer wider than a face", "distance may be too large"),
    ("fillet too large", "radius may be too large"),
    ("corner: end", "cannot end the rounding at a vertex"),
    ("corner: join", "cannot join the roundings"),
    ("corner: in part", "only in part"),
    ("invalid result", "invalid"),
    ("invalid result", "self-intersect"),
    ("invalid result", "unorientable"),
    ("leaves the body open", "leave the body open"),
    ("taken (its result)", "taken (its result)"),
]

HEADER = ["spec", "corpus", "content", "item", "name", "type", "sizes", "edges", "faces", "class",
          "outcome", "input", "before", "after", "distance_before", "distance_after", "states", "error"]


def error_class(error):
    for name, text in CLASSES:
        if text in error:
            return name
    # (An exception out of OCCT, its type or nothing after the colon.)
    if error.startswith(("fillet:", "chamfer:")):
        return "exception"
    return "other"


def resolve(value, values):
    """A size value: a number, or a parameter's value by its name."""
    if isinstance(value, str) and value in values:
        return values[value]
    return value


def sets_of(definition):
    sets = definition.get("sets")
    return [definition] if sets is None else sets


def sizes(definition, values):
    """The sets' sizes, short: `constant:radius=3`."""
    out = []
    for s in sets_of(definition):
        size = s.get("size", s)
        if not isinstance(size, dict):
            out.append(str(resolve(size, values)))
            continue
        kind = size.get("type", "")
        parts = []
        for key in ("radius", "length", "distance", "distance1", "distance2", "start", "end", "angle"):
            if key in size:
                v = resolve(size[key], values)
                if isinstance(v, float):
                    v = round(v, 6)
                parts.append(f"{key}={v}")
        out.append(f"{kind}:{','.join(parts)}" if kind else ",".join(parts))
    return ";".join(out)


def count(definition, key):
    return sum(len(s.get(key, []) or []) for s in sets_of(definition))


def outcomes(out):
    """(content id, design label, item index) -> the item's outcome."""
    found = {}
    for path in glob.glob(os.path.join(out, "reports", "*.json")):
        cid = os.path.basename(path)[:8]
        try:
            with open(path, encoding="utf-8") as f:
                report = json.load(f)
        except (OSError, ValueError):
            continue
        for design in report.get("designs", []):
            label = design.get("label", "")
            for item in design.get("items", []):
                found[(cid, label, item.get("index"))] = item.get("outcome", "")
    return found


def specs(out):
    """Every spec of the dump: (content id, its id, its line)."""
    for path in sorted(glob.glob(os.path.join(out, "specs", "*", "*.jsonl"))):
        cid = os.path.basename(os.path.dirname(path))
        with open(path, encoding="utf-8") as f:
            for line in f:
                try:
                    spec = json.loads(line)
                except ValueError:
                    continue
                spec_input = spec.get("files", {}).get("input", "")
                spec_id = cid + "/" + (spec_input[: -len(".shape")] if spec_input else "?")
                yield cid, spec_id, spec


def index(out):
    files = {}
    with open(os.path.join(out, "files.tsv"), encoding="utf-8") as f:
        for line in f:
            cid, corpus, _ = line.rstrip("\n").split("\t", 2)
            files[cid] = corpus
    done = {}
    for path in glob.glob(os.path.join(out, "done", "*")):
        with open(path, encoding="utf-8") as f:
            fields = f.read().split("\t")
        if len(fields) >= 4:
            done[fields[0]] = (int(fields[2]), int(fields[3]))
    results = outcomes(out)
    rows = []
    for cid, spec_id, spec in specs(out):
        definition = spec.get("def", {})
        values = spec.get("values", {})
        spec_files = spec.get("files", {})
        error = " ".join(str(spec.get("error", "")).split())
        distances = spec.get("sig_distance", {})
        rows.append([
            spec_id,
            files.get(cid, ""),
            cid,
            str(spec.get("item", "")),
            str(spec.get("name", "")),
            str(spec.get("type", "")),
            sizes(definition, values),
            str(count(definition, "edges")),
            str(count(definition, "faces")),
            error_class(error),
            results.get((cid, spec.get("file", ""), spec.get("item")), "linked"),
            os.path.join(cid, spec_files["input"]) if spec_files.get("input") else "",
            os.path.join(cid, spec_files["before"]) if spec_files.get("before") else "",
            os.path.join(cid, spec_files["after"]) if spec_files.get("after") else "",
            "%.3g" % distances["before"] if "before" in distances else "",
            "%.3g" % distances["after"] if "after" in distances else "",
            ",".join(str(s) for s in spec.get("states") or []),
            error,
        ])
    with open(os.path.join(out, "index.tsv"), "w", encoding="utf-8") as f:
        f.write("\t".join(HEADER) + "\n")
        for row in rows:
            f.write("\t".join(row) + "\n")

    lines = []
    statuses = collections.Counter(status for status, _ in done.values())
    lines.append(f"files: {len(files)} distinct, {len(done)} run "
                 f"(exit status: {', '.join(f'{k}: {v}' for k, v in sorted(statuses.items()))}), "
                 f"{sum(t for _, t in done.values())} s in all")
    by_corpus = collections.Counter(files.values())
    lines.append("by corpus: " + ", ".join(f"{k} {v}" for k, v in sorted(by_corpus.items())))
    lines.append(f"specs: {len(rows)} in {len({r[2] for r in rows})} files")
    items = {}
    for r in rows:
        items.setdefault((r[2], r[3], r[4]), r)
    for title, chosen in (("specs", rows), ("items (first spec of each)", list(items.values()))):
        lines.append("")
        lines.append(f"{title} by class and the item's outcome:")
        table = collections.defaultdict(collections.Counter)
        for r in chosen:
            table[r[9]][r[10] or "?"] += 1
        kinds = sorted({k for c in table.values() for k in c})
        lines.append("  " + "class".ljust(26) + "".join(k.rjust(12) for k in kinds) + "total".rjust(8))
        for name, c in sorted(table.items(), key=lambda kv: -sum(kv[1].values())):
            lines.append("  " + name.ljust(26) + "".join(str(c.get(k, 0)).rjust(12) for k in kinds)
                         + str(sum(c.values())).rjust(8))
    with open(os.path.join(out, "summary.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


def number(value, values):
    v = resolve(value, values)
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return float(v)
    raise ValueError(f"size {value!r} is not a number")


def arguments(out, cid, spec):
    """The tool's arguments for a spec; ValueError when it cannot run it."""
    definition = spec.get("def", {})
    values = spec.get("values", {})
    files = spec.get("files", {})
    if not files.get("input"):
        raise ValueError("no input body")
    kind = definition.get("type", spec.get("type", ""))
    args = ["--input", os.path.join(out, "specs", cid, files["input"])]
    if files.get("after"):
        args += ["--after", os.path.join(out, "specs", cid, files["after"])]
    args.append(kind)
    if kind == "fillet":
        args += ["--corners", "rolling" if definition.get("rolling_ball_corners", True) else "setback"]
    elif definition.get("corner"):
        args += ["--corners", str(definition["corner"])]
    for s in sets_of(definition):
        size = s.get("size")
        if size is None and "radius" in s:
            size = {"type": "constant", "radius": s["radius"]}
        if not isinstance(size, dict):
            raise ValueError(f"size {size!r} not run here")
        size_kind = size.get("type", "")
        if size_kind == "constant":
            a, b = number(size.get("radius"), values), 0.0
        elif size_kind == "chord_length":
            a, b = number(size.get("length"), values), 0.0
        elif size_kind == "equal_distance":
            a, b = number(size.get("distance"), values), 0.0
        elif size_kind == "two_distances":
            a, b = number(size.get("distance1"), values), number(size.get("distance2"), values)
        elif size_kind == "distance_angle":
            a, b = number(size.get("distance"), values), number(size.get("angle"), values)
        else:
            raise ValueError(f"size {size_kind} not run here")
        if s.get("continuity") == "curvature" or s.get("all_edges"):
            raise ValueError("curvature continuity or all edges not run here")
        args += ["--set", size_kind, repr(a), repr(b),
                 "chain" if s.get("tangent_chain", True) else "no-chain"]
        for e in s.get("edges", []) or []:
            args += ["--edge", e]
        for face in s.get("faces", []) or []:
            args += ["--face", face]
        if s.get("reference_face"):
            args += ["--reference", s["reference_face"]]
        if s.get("flip"):
            args.append("--flip")
    return args


def run_one(tool, out, cid, spec_id, spec, timeout, extra):
    """(spec id, result, fields) of one run; its output in OUT/runs/."""
    try:
        args = arguments(out, cid, spec)
    except ValueError as e:
        return spec_id, "SKIPPED", {"error": str(e)}
    try:
        done = subprocess.run([tool] + extra + args, capture_output=True, text=True, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        return spec_id, "TIMEOUT", {}
    with open(os.path.join(out, "runs", spec_id.replace("/", "__") + ".txt"), "w", encoding="utf-8") as f:
        f.write(done.stdout + done.stderr)
    fields = {}
    for line in done.stdout.splitlines():
        key, _, value = line.partition(": ")
        fields[key] = value
    if "result" not in fields:
        tail = (done.stderr.strip().splitlines() or [f"exit status {done.returncode}"])[-1]
        return spec_id, "CRASH", {"error": tail}
    return spec_id, fields["result"], fields


def run(out, tool, classes, chosen, jobs, timeout, extra):
    rows = {}
    with open(os.path.join(out, "index.tsv"), encoding="utf-8") as f:
        next(f)
        for line in f:
            row = line.rstrip("\n").split("\t")
            rows[row[0]] = row
    todo = []
    for cid, spec_id, spec in specs(out):
        row = rows.get(spec_id)
        if row is None:
            continue
        if (classes and row[9] not in classes) or (chosen and spec_id not in chosen):
            continue
        todo.append((cid, spec_id, spec))
    os.makedirs(os.path.join(out, "runs"), exist_ok=True)
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = [pool.submit(run_one, tool, out, cid, spec_id, spec, timeout, extra)
                   for cid, spec_id, spec in todo]
        for future in concurrent.futures.as_completed(futures):
            spec_id, result, fields = future.result()
            results.append((spec_id, result, fields))
            print(f"{spec_id}\t{result}\t{fields.get('relative', '')}\t{fields.get('error', '')}", flush=True)
    # The rows of earlier runs of other specs are kept.
    path = os.path.join(out, "results.tsv")
    lines = {}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            next(f, None)
            for line in f:
                lines[line.split("\t", 1)[0]] = line
    for spec_id, result, fields in results:
        lines[spec_id] = "\t".join([spec_id, rows[spec_id][9], result] +
                                   [fields.get(k, "") for k in ("faces", "file_faces", "change", "file_change",
                                                                "relative")] +
                                   [" ".join(fields.get("error", "").split())]) + "\n"
    with open(path, "w", encoding="utf-8") as f:
        f.write("spec\tclass\tresult\tfaces\tfile_faces\tchange\tfile_change\trelative\terror\n")
        for spec_id in sorted(lines):
            f.write(lines[spec_id])
    tally = collections.Counter(result for _, result, _ in results)
    print(", ".join(f"{k} {v}" for k, v in sorted(tally.items())) + f" of {len(results)}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("out")
    parser.add_argument("--run", metavar="TOOL")
    parser.add_argument("--class", dest="classes", action="append", default=[])
    parser.add_argument("--spec", action="append", default=[])
    parser.add_argument("--jobs", type=int, default=3)
    parser.add_argument("--timeout", type=float, default=600)
    parser.add_argument("--describe", action="store_true")
    a = parser.parse_args()
    if a.run:
        run(a.out, a.run, set(a.classes), set(a.spec), a.jobs, a.timeout, ["--describe"] if a.describe else [])
    else:
        index(a.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
