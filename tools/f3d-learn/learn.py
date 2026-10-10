# SPDX-License-Identifier: MIT
"""Hypothesis tester for the .f3d import's learning dump (mitcad#96).

The import writes one JSON line per modelling item whose definition the
file's history settled (``mitcad-cli import-f3d --learn DIR`` or
``MITCAD_IMPORT_LEARN=DIR``; core/import/README.md, "Learning the
undecoded inputs"). This tool reads those lines only (no geometry) and
tests rules from the item's raw record to the accepted answer.

    learn.py stats                         examples per item type, cost
    learn.py show   --kind T [--n N] [--item FILE:INDEX] [--path P]
    learn.py fields --kind T --path P      every gap field of one object
    learn.py survey --kind T [--path P] [--target X]
    learn.py test   --kind T --target X --expr 'PYTHON EXPRESSION'
    learn.py test   --kind T --check 'BOOLEAN EXPRESSION'
    learn.py test   --kind T --rule rule.py     (defines predict(ex) or check(ex))
    learn.py search --kind T --target X [--families eq,map,idin,idset,bits]

search and survey run on --jobs processes (default: the cores less four;
the output does not depend on it).

--data DIR (default: $MITCAD_IMPORT_LEARN) is the dump directory with
settled/ and unsettled/. Field addresses inside an object of the record:

    @N      byte N of the object
    gK+N    byte N of gap K (gaps: the bytes no token explains, after the
            header and root part, split at references, strings and the
            sub-chunk start), gK-N: N bytes before the gap's end
    rK      the object id of the K-th reference token
    tail+N  byte N after a recipe's type string, when the decoder retained
            its framing (opaque bytes; their meaning remains unverified)

Readings: u8 u16 u32 i32 u64 f64 (little-endian), and sign (of the f64).
Python 3.8+, standard library only.
"""

import argparse
import glob
import json
import math
import multiprocessing
import os
import struct
import sys
from collections import Counter, defaultdict
from concurrent.futures import ProcessPoolExecutor

READINGS = {
    "u8": ("<B", 1),
    "u16": ("<H", 2),
    "u32": ("<I", 4),
    "i32": ("<i", 4),
    "u64": ("<Q", 8),
    "f64": ("<d", 8),
}


# ---------------------------------------------------------------------------
# The dataset


class Obj:
    """One object of an item's record."""

    def __init__(self, raw):
        self.raw = raw
        self.path = raw["path"]
        self.id = raw["id"]
        self.cls = raw["class"]
        self.version = raw.get("version", 0)
        self.b = bytes.fromhex(raw["hex"]) if "hex" in raw else None
        self.recipe = raw.get("recipe") or {}
        # A "reference" to object 0 is as likely a u32 1 followed by zeros
        # (object 0 is the settings object): its bytes stay in the gaps.
        self.tokens = [t for t in raw.get("tokens", []) if not (t["t"] == "ref" and t.get("id") == 0)]
        self._gaps = None

    @property
    def gaps(self):
        """[(start, end)] of the bytes no token explains."""
        if self._gaps is None:
            out = []
            if self.b is not None:
                covered = []
                cuts = set()
                for t in self.tokens:
                    if t["t"] == "sub":
                        cuts.add(t["at"])
                    else:
                        covered.append((t["at"], t["end"]))
                covered.sort()
                p = 0
                for a, e in covered:
                    if a > p:
                        out.extend(split(p, a, cuts))
                    p = max(p, e)
                if p < len(self.b):
                    out.extend(split(p, len(self.b), cuts))
            self._gaps = out
        return self._gaps

    def refs(self):
        return [t["id"] for t in self.tokens if t["t"] == "ref"]

    def ref_tokens(self):
        return [t for t in self.tokens if t["t"] == "ref"]

    def offset(self, addr):
        """The byte offset of an address, or None."""
        if addr.startswith("tail+"):
            start = self.recipe.get("tail_offset")
            n = int(addr[5:])
            if self.b is None or not isinstance(start, int) or start < 0 or n < 0:
                return None
            p = start + n
            return p if p < len(self.b) else None
        if addr.startswith("@"):
            return int(addr[1:])
        if addr.startswith("g"):
            sign = "+" if "+" in addr else "-"
            k, n = addr[1:].split(sign)
            k, n = int(k), int(n)
            if k >= len(self.gaps):
                return None
            a, e = self.gaps[k]
            p = a + n if sign == "+" else e - n
            return p if a <= p < e else None
        return None

    def read(self, addr, kind="u32"):
        if addr.startswith("r"):
            refs = self.refs()
            k = int(addr[1:])
            return refs[k] if k < len(refs) else None
        if self.b is None:
            return None
        p = self.offset(addr)
        if p is None:
            return None
        if kind == "sign":
            v = self.read(addr, "f64")
            return None if v is None or v != v else (v > 0) - (v < 0)
        fmt, n = READINGS[kind]
        if p < 0 or p + n > len(self.b):
            return None
        return struct.unpack_from(fmt, self.b, p)[0]


def split(a, e, cuts):
    out = []
    for c in sorted(cuts):
        if a < c < e:
            out.append((a, c))
            a = c
    out.append((a, e))
    return out


class Example:
    """One line of the dump."""

    def __init__(self, raw):
        self.raw = raw
        self.kind = raw.get("type") or "?"
        self.file = raw.get("file")
        self.item = raw.get("item")
        self.settled = raw.get("status") == "settled"
        rec = raw.get("record") or {}
        self.objects = {}
        for o in rec.get("objects", []):
            self.objects[o["path"]] = Obj(o)
        self.version = raw.get("class_version")
        ctx = raw.get("context") or {}
        self.sketches = ctx.get("sketches", {})
        self.bodies = ctx.get("bodies", [])
        self.edges = ctx.get("edges", {})
        self.candidates = raw.get("candidates", [])
        self.answer = raw.get("answer")
        self.known = raw.get("known") or {}

    @property
    def key(self):
        return "%s:%s" % (self.file, self.item)

    def obj(self, path):
        return self.objects.get(path)

    def read(self, path, addr, kind="u32"):
        o = self.objects.get(path)
        return None if o is None else o.read(addr, kind)

    # The sketch the answer (or the candidates) use.
    def sketch(self):
        defs = (self.answer or {}).get("defs") or []
        for d in defs + [d for c in self.candidates for d in c["defs"]]:
            for p in d.get("profiles", []) or []:
                s = p.get("sketch")
                if s in self.sketches:
                    return self.sketches[s]
        return next(iter(self.sketches.values()), None)

    def regions(self):
        s = self.sketch()
        return s["regions"] if s else []

    def curves(self):
        s = self.sketch()
        return (s or {}).get("curves", {})


def load(data, kinds=None, unsettled=False):
    """The examples of the dump directory (settled ones, or the unsettled)."""
    sub = "unsettled" if unsettled else "settled"
    out = []
    for f in sorted(glob.glob(os.path.join(data, sub, "*.jsonl"))):
        with open(f, encoding="utf-8") as fh:
            for line in fh:
                if not line.strip():
                    continue
                raw = json.loads(line)
                if kinds and raw.get("type") not in kinds:
                    continue
                if not raw.get("candidates"):
                    continue
                out.append(Example(raw))
    return out


# ---------------------------------------------------------------------------
# Answers (targets)


def defs_of(answer):
    return (answer or {}).get("defs") or []


def answer_regions(ex, answer=None):
    """The selected regions: a sorted tuple of region indices (the
    sketch's `regions` list), None when the answer names none."""
    answer = ex.answer if answer is None else answer
    out = set()
    for d in defs_of(answer):
        for p in d.get("profiles", []) or []:
            if isinstance(p.get("region"), int):
                out.add(p["region"])
            else:
                # A region not among the sketch's (a face's profile).
                return None
    return tuple(sorted(out)) if out else None


def answer_curves(ex, answer=None):
    """The file curve ids of the selected regions (outer and hole loops)."""
    regions = answer_regions(ex, answer)
    if regions is None:
        return None
    rs = ex.regions()
    out = set()
    for i in regions:
        if i < len(rs):
            for loop in [rs[i]["outer"]] + rs[i]["holes"]:
                out.update(loop)
    return tuple(sorted(out))


def answer_edges(ex, answer=None):
    out = set()
    for d in defs_of(ex.answer if answer is None else answer):
        for s in d.get("sets", []) or []:
            out.update(s.get("edges", []) or [])
    return tuple(sorted(out)) if out else None


def first_def(ex, answer=None):
    """The definition of the item's own feature (not a sketch made for it)."""
    defs = defs_of(ex.answer if answer is None else answer)
    for d in defs:
        if d.get("type") != "sketch":
            return d
    return defs[0] if defs else None


def def_field(name):
    def f(ex, answer=None):
        d = first_def(ex, answer)
        if d is None:
            return None
        v = d.get(name)
        return canon(v)

    return f


def canon(v):
    """A hashable, comparable form of a JSON value."""
    if isinstance(v, dict):
        return tuple(sorted((k, canon(x)) for k, x in v.items()))
    if isinstance(v, list):
        return tuple(canon(x) for x in v)
    return v


def extent_type(ex, answer=None):
    d = first_def(ex, answer)
    e = (d or {}).get("extent")
    if not isinstance(e, dict):
        return None
    return "%s%s" % (e.get("type"), "+both" if e.get("both_sides") or e.get("symmetric") else "")


TARGETS = {
    "regions": answer_regions,
    "region_count": lambda ex, a=None: (lambda r: None if r is None else len(r))(answer_regions(ex, a)),
    "curves": answer_curves,
    "edges": answer_edges,
    "edge_count": lambda ex, a=None: (lambda r: None if r is None else len(r))(answer_edges(ex, a)),
    "flip": def_field("flip"),
    "operation": def_field("operation"),
    "extent": extent_type,
    "participants": def_field("participants"),
    "axis": def_field("axis"),
    "def_type": def_field("type"),
    "rank": lambda ex, a=None: ex.raw.get("accepted"),
    # Whether the answer was a guess the history had to check (fillet
    # edges not found by their names, profiles not decoded, ...).
    "guess": lambda ex, a=None: ((ex.answer if a is None else a) or {}).get("guess"),
}

# Targets that are sets of entities with ids, and how to get the ids of
# the entities in and out of the answer.
SET_TARGETS = ("regions", "curves", "edges")


def target(ex, name, answer=None):
    if name in TARGETS:
        return TARGETS[name](ex, answer)
    # def.<key>: any key of the item's definition.
    if name.startswith("def."):
        return def_field(name[4:])(ex, answer)
    raise SystemExit("unknown target %r; known: %s, def.<key>" % (name, ", ".join(TARGETS)))


def same(pred, actual):
    if isinstance(pred, (set, frozenset, list, tuple)) and isinstance(actual, tuple):
        try:
            return tuple(sorted(pred)) == tuple(sorted(actual))
        except TypeError:
            return tuple(pred) == actual
    if isinstance(pred, float) or isinstance(actual, float):
        try:
            return abs(float(pred) - float(actual)) <= 1e-9 * max(1.0, abs(float(actual)))
        except (TypeError, ValueError):
            return False
    return pred == actual


# ---------------------------------------------------------------------------
# Entity ids (for the idin / idset families)


def curve_ids(ex, curve_list, attr):
    cs = ex.curves()
    out = set()
    for c in curve_list:
        e = cs.get(c)
        if e is None:
            continue
        if attr == "object":
            out.add(e.get("object"))
        elif attr == "local":
            out.add(int(c[1:]) if c[1:].isdigit() else None)
        else:
            out.add(e.get("attrs", {}).get(attr))
    out.discard(None)
    return out


def all_curve_ids(ex):
    return [c for c in ex.curves() if c.startswith("c")]


ID_ATTRS = ("crv_primary_id", "crv_secondary_id", "object", "local")


def id_split(ex, tname, attr):
    """(ids of the answer's entities, ids of the other candidates')."""
    if tname in ("regions", "curves"):
        inside = answer_curves(ex)
        if inside is None:
            return None
        a = curve_ids(ex, inside, attr)
        others = [c for c in all_curve_ids(ex) if c not in set(inside)]
        o = curve_ids(ex, others, attr)
        # Ids both sides have (a secondary id 0) tell nothing.
        both = a & o
        a, o = a - both - {0}, o - both - {0}
        return (a, o) if a else None
    return None


# ---------------------------------------------------------------------------
# Rules


class Env(dict):
    """The names a rule expression sees."""


def env_for(ex):
    e = Env()
    e["ex"] = ex
    e["obj"] = ex.obj
    e["regions"] = ex.regions()
    e["curves"] = ex.curves()
    e["target"] = lambda name: target(ex, name)
    for k in READINGS:
        e[k] = (lambda kind: lambda path, addr: ex.read(path, addr, kind))(k)
    e["sign"] = lambda path, addr: ex.read(path, addr, "sign")
    e["refs"] = lambda path: (ex.obj(path).refs() if ex.obj(path) else None)
    e["u32s"] = lambda path: values_in(ex.obj(path), "u32")
    e["curve_attr"] = lambda c, a: (ex.curves().get(c) or {}).get("attrs", {}).get(a)
    e["curve_by"] = lambda a, v: [c for c, x in ex.curves().items() if x.get("attrs", {}).get(a) == v]
    e["regions_with_curves"] = lambda cs: tuple(
        i for i, r in enumerate(ex.regions()) if set(r["outer"]) <= set(cs)
    )
    e["math"] = math
    e["struct"] = struct
    return e


def values_in(o, kind):
    """Every reading of a kind at every byte offset of an object's gaps."""
    if o is None or o.b is None:
        return []
    fmt, n = READINGS[kind]
    out = []
    for a, e in o.gaps:
        for p in range(a, e - n + 1):
            out.append(struct.unpack_from(fmt, o.b, p)[0])
    return out


def compile_rule(args):
    """predict(ex) -> value or None, or check(ex) -> True/False/None."""
    if args.rule:
        ns = {}
        with open(args.rule, encoding="utf-8") as fh:
            exec(compile(fh.read(), args.rule, "exec"), ns)
        if "check" in ns:
            return None, ns["check"]
        return ns["predict"], None
    if args.check:
        code = compile(args.check, "<check>", "eval")
        return None, lambda ex: eval(code, {"__builtins__": __builtins__}, env_for(ex))
    if args.expr:
        code = compile(args.expr, "<expr>", "eval")
        return lambda ex: eval(code, {"__builtins__": __builtins__}, env_for(ex)), None
    raise SystemExit("give --expr, --check or --rule")


def run_rule(ex, predict, check, tname):
    """('explained' | 'contradicted' | 'n/a', predicted, actual)."""
    try:
        if check is not None:
            r = check(ex)
            if r is None:
                return "n/a", None, None
            return ("explained" if r else "contradicted"), r, None
        actual = target(ex, tname)
        if actual is None:
            return "n/a", None, None
        p = predict(ex)
        if p is None:
            return "n/a", None, actual
        return ("explained" if same(p, actual) else "contradicted"), p, actual
    except (TypeError, ValueError, KeyError, IndexError, AttributeError, ZeroDivisionError, struct.error) as e:
        return "error", repr(e), None


def cmd_test(args, examples):
    predict, check = compile_rule(args)
    if check is None and not args.target:
        raise SystemExit("--target is needed with --expr or --rule predict()")
    counts = Counter()
    bad = []
    for ex in examples:
        r, p, a = run_rule(ex, predict, check, args.target)
        counts[r] += 1
        if r in ("contradicted", "error"):
            bad.append((ex, p, a, r))
    n = len(examples)
    print("kind %s target %s: %d examples" % (args.kind, args.target or "(check)", n))
    for k in ("explained", "contradicted", "n/a", "error"):
        print("  %-12s %6d" % (k, counts[k]))
    for ex, p, a, r in bad[: args.n]:
        print("--- %s %s (%s v%s): %s predicted %s, answer %s" % (r, ex.key, ex.raw.get("name"), ex.version,
                                                                   r, short(p), short(a)))
        for path in args.path or []:
            show_object(ex.obj(path), path)
    if args.unsettled and predict is not None:
        un = load(args.data, [args.kind], unsettled=True)
        c2 = Counter()
        for ex in un:
            try:
                p = predict(ex)
            except Exception:  # noqa: BLE001 - a rule may fail on any example
                c2["error"] += 1
                continue
            if p is None:
                c2["n/a"] += 1
                continue
            ranks = [i for i, c in enumerate(ex.candidates)
                     if (lambda v: v is not None and same(p, v))(target(ex, args.target, c))]
            c2["among candidates" if ranks else "not among candidates"] += 1
        print("unsettled items (%d): %s" % (len(un), dict(c2)))
    return 0 if counts["contradicted"] == 0 and counts["error"] == 0 else 1


def short(v, n=160):
    s = json.dumps(v, default=str) if not isinstance(v, str) else v
    return s if len(s) <= n else s[:n] + "..."


# ---------------------------------------------------------------------------
# Showing examples


def show_object(o, path=None):
    if o is None:
        print("  (no object %s)" % path)
        return
    print("  %s  id %s class %s v%s len %s" % (o.path, o.id, o.cls[:8], o.version, o.raw.get("len")))
    if o.b is None:
        print("    (bytes not kept)")
        return
    if o.recipe:
        print("    recipe %s entities %s secondary %s" % (
            o.recipe.get("kind"), short(o.recipe.get("entities")),
            short(o.recipe.get("secondary_entities"))))
        print("    opaque recipe tail @%s (%s): %s" % (
            o.recipe.get("tail_offset"), o.recipe.get("tail_interpretation", "unverified"),
            short(o.recipe.get("tail_hex"))))
    gaps = o.gaps
    items = [(t["at"], "t", t) for t in o.tokens] + [(a, "g", (k, a, e)) for k, (a, e) in enumerate(gaps)]
    items.sort(key=lambda x: (x[0], x[1] == "g"))
    for _, what, x in items:
        if what == "t":
            t = x
            if t["t"] == "ref":
                print("    %5d ref -> %s %s" % (t["at"], t["id"], t["class"]))
            elif t["t"] == "str16":
                print("    %5d str16 %r" % (t["at"], t["text"]))
            elif t["t"] == "root":
                print("    %5d root refs %s attrs %s" % (t["at"], t.get("refs"), t.get("attrs")))
            elif t["t"] == "sub":
                print("    %5d sub-chunk" % t["at"])
        else:
            k, a, e = x
            print("    %5d g%d [%d bytes] %s" % (a, k, e - a, o.b[a:e].hex()))
            print("          %s" % readings_line(o.b, a, e))


def readings_line(b, a, e):
    """u32 and f64 readings at 4-byte steps of a gap (for reading it)."""
    parts = []
    p = a
    while p + 4 <= e and len(parts) < 24:
        u = struct.unpack_from("<I", b, p)[0]
        s = "+%d:%d" % (p - a, u)
        if p + 8 <= e:
            d = struct.unpack_from("<d", b, p)[0]
            if d == d and d != 0 and 1e-9 < abs(d) < 1e9:
                s += "/%.6g" % d
        parts.append(s)
        p += 4
    return " ".join(parts)


def cmd_show(args, examples):
    sel = examples
    if args.item:
        sel = [e for e in examples if e.key == args.item]
    for ex in sel[: args.n]:
        print("=== %s %s %s v%s %s; accepted %s of %d; outcome %s" % (
            ex.key, ex.raw.get("name"), ex.kind, ex.version, ex.raw.get("status"),
            ex.raw.get("accepted"), len(ex.candidates), ex.raw.get("outcome")))
        print("known:", short(ex.known.get("detail"), 600))
        for name in ("regions", "curves", "edges", "flip", "operation", "extent", "participants", "axis"):
            v = target(ex, name)
            if v is not None:
                print("answer %s: %s" % (name, short(v, 400)))
        s = ex.sketch()
        if s:
            print("sketch T%s: %d regions, %d curves" % (s.get("index"), len(s["regions"]), len(s.get("curves", {}))))
            for i, r in enumerate(s["regions"][:40]):
                ids = [ (s.get("curves", {}).get(c) or {}).get("attrs", {}).get("crv_primary_id") for c in r["outer"]]
                print("  region %d area %.4g outer %s primary %s holes %s" % (i, r["area"], r["outer"], ids, r["holes"]))
        paths = args.path or list(ex.objects)
        for p in paths:
            show_object(ex.obj(p), p)
    return 0


def cmd_fields(args, examples):
    """Every field of one object path over the examples, one row each."""
    path = (args.path or ["item"])[0]
    kind = args.reading
    width = READINGS[kind][1]
    for ex in examples[: args.n]:
        o = ex.obj(path)
        if o is None or o.b is None:
            continue
        cells = []
        for k, (a, e) in enumerate(o.gaps):
            vals = [str(struct.unpack_from(READINGS[kind][0], o.b, p)[0]) for p in range(a, e - width + 1, width)]
            cells.append("g%d[%s]" % (k, ",".join(vals)))
        tv = target(ex, args.target) if args.target else None
        print("%s v%s %s | %s" % (ex.key, ex.version, short(tv, 60) if args.target else "", " ".join(cells)))
    return 0


# ---------------------------------------------------------------------------
# Field enumeration


def addresses(o, head=64, tail=32):
    """The gap addresses of an object: gK+N for N < head, gK-N for N <= tail."""
    out = []
    for k, (a, e) in enumerate(o.gaps):
        n = e - a
        for i in range(min(n, head)):
            out.append("g%d+%d" % (k, i))
        for i in range(1, min(n, tail) + 1):
            out.append("g%d-%d" % (k, i))
    for k in range(len(o.refs())):
        out.append("r%d" % k)
    return out


def field_table(examples, paths=None, readings=("u8", "u16", "u32", "i32", "u64", "f64", "sign", "ref")):
    """{(path, addr, reading): {example index: value}} over the examples
    (`ref`: the rK addresses)."""
    table = defaultdict(dict)
    for i, ex in enumerate(examples):
        for path, o in ex.objects.items():
            if paths and path not in paths:
                continue
            if o.b is None:
                continue
            for addr in addresses(o):
                if addr.startswith("r"):
                    if "ref" in readings:
                        v = o.read(addr)
                        if v is not None:
                            table[(path, addr, "ref")][i] = v
                    continue
                for r in readings:
                    if r == "ref":
                        continue
                    v = o.read(addr, r)
                    if v is None or (isinstance(v, float) and v != v):
                        continue
                    table[(path, addr, r)][i] = v
    return table


def common_paths(examples, share=0.5):
    c = Counter(p for ex in examples for p in ex.objects)
    return [p for p, n in sorted(c.items(), key=lambda x: (-x[1], x[0])) if n >= share * len(examples)]


# ---------------------------------------------------------------------------
# Parallel work: the examples are loaded once; forked workers share them
# (where fork is not available, the work runs in this process).

ALL_READINGS = ("u8", "u16", "u32", "i32", "u64", "f64", "sign", "ref")

# What the workers read (set before the pool forks).
_WORK = {}


def default_jobs():
    return max(1, (os.cpu_count() or 1) - 4)


def parallel_map(fn, tasks, jobs):
    """[fn(t) for t in tasks], over `jobs` forked processes when there are
    several tasks; the results in the tasks' order."""
    if jobs <= 1 or len(tasks) <= 1:
        return [fn(t) for t in tasks]
    try:
        ctx = multiprocessing.get_context("fork")
    except ValueError:
        return [fn(t) for t in tasks]
    with ProcessPoolExecutor(max_workers=min(jobs, len(tasks)), mp_context=ctx) as pool:
        return list(pool.map(fn, tasks, chunksize=1))


# ---------------------------------------------------------------------------
# Search


def _search_fields(task):
    """The eq, map, idin and bits rows of one object path and reading."""
    path, reading = task
    w = _WORK
    exs, acts, tname, families, n = w["exs"], w["acts"], w["tname"], w["families"], w["n"]
    is_set = tname in SET_TARGETS
    table = field_table(exs, [path], (reading,))
    results = []
    # eq: the field's value is the answer (scalars, counts).
    if "eq" in families:
        for key, vals in table.items():
            if len(vals) < 2 or (is_set and key[2] in ("sign", "f64", "ref")):
                continue
            ok = sum(1 for i, v in vals.items() if same(v, len(acts[i]) if is_set else acts[i]))
            bad = len(vals) - ok
            if ok:
                results.append((ok - bad, "eq", key, ok, bad, n - len(vals),
                                "value == %s" % ("number of " + tname if is_set else tname)))
    # map: each value of the field stands for one answer class. Scored
    # leave-one-out: an example is explained when the other examples with
    # its value say its class, not applicable when none has its value (so
    # a field that differs everywhere explains nothing).
    if "map" in families and w["classes"] > 1:
        for key, vals in table.items():
            distinct = set(vals.values())
            if len(distinct) < 2 or len(distinct) > max(2, len(vals) // 3):
                continue
            by = defaultdict(Counter)
            cls = {}
            for i, v in vals.items():
                cls[i] = canon(len(acts[i]) if is_set else acts[i])
                by[v][cls[i]] += 1
            ok = bad = 0
            for i, v in vals.items():
                c = by[v].copy()
                c[cls[i]] -= 1
                c = +c
                if not c:
                    continue
                if majority(c) == cls[i]:
                    ok += 1
                else:
                    bad += 1
            # Not better than always saying the commonest class: no signal.
            base = Counter(cls.values()).most_common(1)[0][1]
            if ok > base or (ok and bad == 0 and len(by) > 1):
                mapping = {v: majority(c) for v, c in sorted(by.items(), key=lambda x: repr(x[0]))}
                results.append((ok - 2 * bad - base, "map", key, ok, bad, n - ok - bad,
                                "mapping %s" % short(mapping, 120)))
    # idin: the field holds the id of an entity of the answer, and of no other.
    if "idin" in families and tname in ("regions", "curves") and reading in ("u32", "i32", "u64", "u16"):
        for attr in ID_ATTRS:
            splits = w["splits"][attr]
            for key, vals in table.items():
                ok = bad = 0
                for i, v in vals.items():
                    s = splits[i]
                    if s is None:
                        continue
                    if v in s[0] and v not in s[1]:
                        ok += 1
                    else:
                        bad += 1
                if ok >= 2:
                    results.append((ok - bad, "idin", key, ok, bad, n - len(vals),
                                    "value is the %s of a curve of the answer, of none other" % attr))
    # bits: bit i of the field <-> region i selected.
    if "bits" in families and tname == "regions" and reading in ("u8", "u16", "u32", "u64"):
        for key, vals in table.items():
            ok = sum(1 for i, v in vals.items() if tuple(j for j in range(64) if v >> j & 1) == acts[i])
            bad = len(vals) - ok
            if ok >= 2:
                results.append((ok - bad, "bits", key, ok, bad, n - len(vals), "bit i <-> region i selected"))
    return results


def _search_idset(path):
    """idset: the ids found anywhere in an object's gaps are the answer's."""
    w = _WORK
    exs, n = w["exs"], w["n"]
    results = []
    for attr in ID_ATTRS:
        splits = w["splits"][attr]
        for r in ("u32", "u64"):
            rec = cont = 0.0
            m = full = clean = 0
            for i, ex in enumerate(exs):
                s = splits[i]
                o = ex.obj(path)
                if s is None or o is None or o.b is None or not s[0]:
                    continue
                vals = set(values_in(o, r))
                hit = len(s[0] & vals) / len(s[0])
                other = len(s[1] & vals) / len(s[1]) if s[1] else 0.0
                rec += hit
                cont += other
                m += 1
                full += hit == 1.0
                clean += hit == 1.0 and other == 0.0
            if m:
                results.append((clean * 2 - (m - full), "idset", (path, "*", r), clean, m - clean, n - m,
                                "the %s of the answer's curves are among the object's %s values: "
                                "mean recall %.3f, mean share of the other curves' ids %.3f, "
                                "all found in %d" % (attr, r, rec / m, cont / m, full)))
    return results


def majority(counter):
    """The commonest key, the smallest (by repr) among equals."""
    best = max(counter.values())
    return min((k for k, c in counter.items() if c == best), key=repr)


def search(examples, tname, families, paths, top, jobs=1):
    actual = [target(ex, tname) for ex in examples]
    idx = [i for i, a in enumerate(actual) if a is not None]
    if not idx:
        print("no example has target %s" % tname)
        return []
    exs = [examples[i] for i in idx]
    acts = [actual[i] for i in idx]
    _WORK.clear()
    _WORK.update(exs=exs, acts=acts, tname=tname, families=families, n=len(idx),
                 classes=len(Counter(canon(a) for a in acts)), splits={})
    if tname in ("regions", "curves") and ({"idin", "idset"} & set(families)):
        for attr in ID_ATTRS:
            _WORK["splits"][attr] = [id_split(ex, tname, attr) for ex in exs]
    # One task per object path and reading, the paths in most examples
    # first (the largest tables).
    counts = Counter(p for ex in exs for p in ex.objects if ex.objects[p].b is not None)
    allpaths = [p for p in sorted(counts, key=lambda p: (-counts[p], p)) if not paths or p in paths]
    tasks = [(p, r) for p in allpaths for r in ALL_READINGS]
    results = [r for part in parallel_map(_search_fields, tasks, jobs) for r in part]
    if "idset" in families and tname in ("regions", "curves"):
        ipaths = paths or common_paths(exs)
        results += [r for part in parallel_map(_search_idset, list(ipaths), jobs) for r in part]
    _WORK.clear()

    # The same field read wider or signed explains the same: the first
    # reading of PREFER only. Sorted completely, so the output does not
    # depend on how the work was split.
    def prefer(r):
        order = PREFER_MAP if r[1] == "map" else PREFER
        return order.index(r[2][2]) if r[2][2] in order else 99

    seen = set()
    out = []
    for r in sorted(results, key=lambda r: (-r[0], prefer(r), r[1], str(r[2]), r[6])):
        k = (r[1], r[2][0], r[2][1], r[3], r[4])
        if k in seen:
            continue
        seen.add(k)
        out.append(r)
    return out[:top]


PREFER = ["u32", "i32", "u16", "u8", "u64", "f64", "sign", "ref"]
# A flag or enum: the narrowest reading that tells the classes apart.
PREFER_MAP = ["u8", "sign", "u16", "u32", "i32", "u64", "f64", "ref"]


def cmd_search(args, examples):
    families = args.families.split(",")
    res = search(examples, args.target, families, args.path, args.top, args.jobs)
    print("kind %s target %s, %d examples; rows: family, field (path, address, reading), "
          "explained, contradicted, not applicable" % (args.kind, args.target, len(examples)))
    for score, fam, key, ok, bad, na, what in res:
        print("%-6s %-55s %6d %6d %6d  %s" % (fam, "%s %s %s" % key, ok, bad, na, what))
    return 0


# ---------------------------------------------------------------------------
# Survey


def _survey_path(path):
    """The survey lines of one object path."""
    w = _WORK
    examples, tgt, args = w["examples"], w["tgt"], w["args"]
    lines = []
    gaps = Counter(len(ex.obj(path).gaps) for ex in examples if ex.obj(path) and ex.obj(path).b)
    lines.append("\n%s: gaps per object %s" % (path, dict(sorted(gaps.items()))))
    sub = [i for i, ex in enumerate(examples) if ex.obj(path)]
    table = field_table([examples[i] for i in sub], [path], readings=(args.reading,))
    rows = []
    for key, vals in table.items():
        dist = Counter(vals.values())
        if len(dist) < 2 and not args.constant:
            continue
        assoc = ""
        if args.target:
            by = defaultdict(Counter)
            for i, v in vals.items():
                t = tgt[sub[i]]
                if t is not None:
                    by[v][canon(t if args.target not in SET_TARGETS else len(t))] += 1
            tot = sum(sum(c.values()) for c in by.values())
            if tot:
                pure = sum(max(c.values()) for c in by.values()) / tot
                assoc = " purity %.2f" % pure
        common = sorted(dist.items(), key=lambda x: (-x[1], repr(x[0])))[:4]
        rows.append((key, len(vals), len(dist), common, assoc))

    def order(r):
        addr = r[0][1]
        if addr[0] != "g":
            return (999, addr)
        k = int(addr[1:].replace("-", "+").split("+")[0])
        return (k, "-" in addr, int(addr.replace("-", "+").split("+")[1]))

    rows.sort(key=order)
    for key, m, d, common, assoc in rows[: args.top * 4]:
        lines.append("  %-10s %-4s in %5d, %5d values, commonest %s%s" % (
            key[1], key[2], m, d, short(common, 70), assoc))
    return lines


def cmd_survey(args, examples):
    paths = args.path or common_paths(examples, args.share)
    print("kind %s: %d examples; class versions %s" % (
        args.kind, len(examples), dict(sorted(Counter(ex.version for ex in examples).items(), key=str))))
    pc = Counter(p for ex in examples for p in ex.objects)
    print("objects (path: share of examples, class, lengths):")
    for p, c in sorted(pc.items(), key=lambda x: (-x[1], x[0]))[: args.top]:
        lens = Counter(ex.obj(p).raw.get("len") for ex in examples if ex.obj(p))
        cls = Counter(ex.obj(p).cls[:8] for ex in examples if ex.obj(p))
        print("  %-60s %5.2f %s lengths %s" % (p, c / len(examples), dict(cls), short(dict(lens.most_common(6)), 90)))
    _WORK.clear()
    _WORK.update(examples=examples, args=args,
                 tgt=[target(ex, args.target) if args.target else None for ex in examples])
    for lines in parallel_map(_survey_path, list(paths), args.jobs):
        for line in lines:
            print(line)
    _WORK.clear()
    return 0


# ---------------------------------------------------------------------------
# Stats


def cmd_stats(args, _examples):
    rows = defaultdict(lambda: [0, 0, 0, 0, 0.0, 0])
    for unsettled in (False, True):
        for ex in load(args.data, None, unsettled):
            r = rows[ex.kind]
            if unsettled:
                r[1] += 1
                r[5] += len(ex.candidates)
            else:
                r[0] += 1
                r[2] += len(ex.candidates)
                r[3] += (ex.raw.get("accepted") or 0) + 1
                r[4] += (ex.raw.get("cost") or {}).get("seconds") or 0.0
    print("%-28s %8s %10s %12s %12s %12s %14s" % (
        "type", "settled", "unsettled", "cand/item", "tried/item", "s/item", "unsettled cand"))
    for k, (s, u, cand, rank, secs, ucand) in sorted(rows.items(), key=lambda x: -(x[1][3] + x[1][5])):
        print("%-28s %8d %10d %12.1f %12.1f %12.2f %14.1f" % (
            k, s, u, cand / max(s, 1), rank / max(s, 1), secs / max(s, 1), ucand / max(u, 1)))
    return 0


# ---------------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["stats", "show", "fields", "survey", "test", "search"])
    ap.add_argument("--data", default=os.environ.get("MITCAD_IMPORT_LEARN"))
    ap.add_argument("--kind", help="item type, e.g. ExtrudeFeature")
    ap.add_argument("--version", type=int, help="only this class version")
    ap.add_argument("--target", help="answer to explain: " + ", ".join(TARGETS) + ", def.<key>")
    ap.add_argument("--expr", help="Python expression predicting the target")
    ap.add_argument("--check", help="Python expression, True when the example agrees")
    ap.add_argument("--rule", help="file defining predict(ex) or check(ex)")
    ap.add_argument("--path", action="append", help="object path(s) of the record")
    ap.add_argument("--item", help="FILE:INDEX of one example")
    ap.add_argument("--n", type=int, default=5, help="examples or counterexamples shown")
    ap.add_argument("--top", type=int, default=30)
    ap.add_argument("--share", type=float, default=0.5, help="survey: paths in at least this share")
    ap.add_argument("--reading", default="u32", choices=list(READINGS) + ["sign"])
    ap.add_argument("--constant", action="store_true", help="survey: list constant fields too")
    ap.add_argument("--families", default="eq,map,idin,idset,bits")
    ap.add_argument("--jobs", type=int, default=default_jobs(),
                    help="search, survey: worker processes (default: the cores less four)")
    ap.add_argument("--unsettled", action="store_true",
                    help="test: also check the rule's predictions on the unsettled items")
    args = ap.parse_args(argv)
    if not args.data:
        raise SystemExit("no dataset: give --data or set MITCAD_IMPORT_LEARN")
    if args.command == "stats":
        return cmd_stats(args, None)
    if not args.kind:
        raise SystemExit("--kind is needed")
    examples = load(args.data, [args.kind])
    if args.version is not None:
        examples = [e for e in examples if e.version == args.version]
    cmd = {"show": cmd_show, "fields": cmd_fields, "survey": cmd_survey, "test": cmd_test, "search": cmd_search}
    return cmd[args.command](args, examples)


if __name__ == "__main__":
    sys.exit(main())
