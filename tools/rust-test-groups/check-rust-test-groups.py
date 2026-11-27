#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Every Rust test file is a module of a test binary (mitcad#2496).

A crate's integration tests are a few test binaries grouped by subject:
`tests/<group>/main.rs`, which Cargo builds as the test binary `<group>`,
declares the files of its folder as modules (`mod <file>;`). Cargo builds
each `tests/<name>.rs` as a binary of its own, which links the crate and all
its dependencies once more, and compiles a file under `tests/<group>/` only
when a module declaration reaches it: a test file no declaration reaches is
never built or run, without any warning. This scan reads every Rust package
under the folders given (a folder with a `Cargo.toml` that has `[package]`)
and reports:
- a file `tests/<name>.rs` that Cargo builds as a test binary of its own
  (a test target the package declares in its `Cargo.toml` is its choice
  and is not reported): it goes into a group folder as a module;
- a `.rs` file in a folder named `tests` (the integration tests' folder,
  or a library's `src/.../tests/` of unit tests) that no target of the
  package reaches through its `mod` declarations (`#[path]` read too),
  nor an `include!`, `include_str!` or `include_bytes!`.
The declarations are read from the sources without a build: comments and
literals are skipped, inline modules followed; `cfg` is not evaluated (a
module of another platform counts as declared).

  check-rust-test-groups.py FOLDER...
      scans the packages under the folders; prints each problem as
      `file: ...` and exits 1 when there is one.
  check-rust-test-groups.py --compare BEFORE AFTER
      compares two builds' tests (each file a line per test: the package,
      the target as `<kind> <name>` and the test, separated by tabs, as
      `cmake -DLIST=... -P core/cargo-tests.cmake -- names <file>` writes
      them). A test of a test binary `<file>` before may be the test
      `<file>::<test>` of a group binary after (the file became a module
      of the group), or the same test of the same binary. Prints the test
      binaries per kind before and after, and each test that is missing
      or new; exits 1 when one is.
  check-rust-test-groups.py --self-test
      runs the scan and the comparison on planted sources and lists.

Run with `python3 -I`; it needs no build (the ctest repo.rust-test-groups).
"""

import argparse
import collections
import os
import posixpath
import re
import sys

# The folders a package's scan does not enter.
SKIPPED = {"target", "build", "node_modules", "__pycache__"}

TOKEN = re.compile(
    r"""
      (?P<space>\s+)
    | (?P<line>//[^\n]*)
    | (?P<block>/\*)
    | (?P<raw>b?r(?P<hashes>\#*)")
    | (?P<string>b?"(?:\\.|[^"\\])*")
    | (?P<char>b?'(?:\\(?:u\{[0-9a-fA-F]+\}|x[0-9a-fA-F]{2}|.)|[^\\'\n])')
    | (?P<ident>r\#[A-Za-z_][A-Za-z0-9_]*|[A-Za-z_][A-Za-z0-9_]*)
    | (?P<punct>.)
    """,
    re.S | re.X,
)


def lex(text):
    """The tokens of Rust source: ("ident", name), ("string", value) and
    ("punct", character); comments, whitespace and characters dropped."""
    tokens = []
    at = 0
    end = len(text)
    while at < end:
        match = TOKEN.match(text, at)
        kind = match.lastgroup
        if kind == "hashes":
            kind = "raw"
        if kind == "block":
            depth = 1
            at = match.end()
            while at < end and depth:
                if text.startswith("/*", at):
                    depth += 1
                    at += 2
                elif text.startswith("*/", at):
                    depth -= 1
                    at += 2
                else:
                    at += 1
            continue
        if kind == "raw":
            close = '"' + match.group("hashes")
            stop = text.find(close, match.end())
            stop = end if stop < 0 else stop
            tokens.append(("string", text[match.end():stop]))
            at = stop + len(close)
            continue
        if kind == "string":
            body = match.group(kind)
            body = body[body.index('"') + 1:-1]
            tokens.append(("string", re.sub(r"\\(.)", r"\1", body, flags=re.S)))
        elif kind == "ident":
            tokens.append(("ident", match.group(kind)))
        elif kind == "punct":
            tokens.append(("punct", match.group(kind)))
        at = match.end()
    return tokens


def declarations(text):
    """The module declarations and inclusions of a source file: a list of
    ("mod", inline, name, path) and ("include", path), where inline is the
    names of the inline modules the declaration is in and path the value
    of its #[path] attribute (or None)."""
    tokens = lex(text)
    found = []
    stack = []  # the open braces: an inline module's name, or None
    path = None
    i = 0
    count = len(tokens)

    def at(k):
        return tokens[k] if k < count else (None, None)

    while i < count:
        kind, value = tokens[i]
        if (kind, value) == ("punct", "#"):
            j = i + 1
            if at(j) == ("punct", "!"):
                j += 1
            if at(j) == ("punct", "["):
                depth = 0
                k = j
                while k < count:
                    if tokens[k] == ("punct", "["):
                        depth += 1
                    elif tokens[k] == ("punct", "]"):
                        depth -= 1
                        if depth == 0:
                            break
                    k += 1
                inner = tokens[j + 1:k]
                if (len(inner) == 3 and inner[0] == ("ident", "path")
                        and inner[1] == ("punct", "=") and inner[2][0] == "string"):
                    path = inner[2][1]
                i = k + 1
                continue
        if (kind, value) == ("ident", "mod") and at(i + 1)[0] == "ident":
            name = at(i + 1)[1]
            if name.startswith("r#"):
                name = name[2:]
            after = at(i + 2)
            if after == ("punct", ";"):
                inline = tuple(n for n in stack if n is not None)
                found.append(("mod", inline, name, path))
                path = None
                i += 3
                continue
            if after == ("punct", "{"):
                stack.append(name)
                path = None
                i += 3
                continue
        if (kind in ("ident",) and value in ("include", "include_str", "include_bytes")
                and at(i + 1) == ("punct", "!") and at(i + 2) == ("punct", "(")
                and at(i + 3)[0] == "string"):
            found.append(("include", at(i + 3)[1]))
            i += 4
            continue
        if kind == "punct":
            if value == "{":
                stack.append(None)
            elif value == "}" and stack:
                stack.pop()
            if value in "{};":
                path = None
        i += 1
    return found


def join(*parts):
    parts = [p for p in parts if p]
    return posixpath.normpath(posixpath.join(*parts)) if parts else ""


def module_dir(file, mod_rs):
    """The folder a file's `mod x;` declarations look in."""
    if mod_rs:
        return posixpath.dirname(file)
    stem = posixpath.splitext(posixpath.basename(file))[0]
    return join(posixpath.dirname(file), stem)


def reach(files, roots):
    """The files of a package (relative paths) its roots reach."""
    reached = set()
    todo = [(root, True) for root in roots if root in files]
    while todo:
        file, mod_rs = todo.pop()
        if file in reached:
            continue
        reached.add(file)
        for found in declarations(files[file]):
            if found[0] == "include":
                target = join(posixpath.dirname(file), found[1].replace("\\", "/"))
                if target in files:
                    reached.add(target)
                continue
            _, inline, name, path = found
            base = join(module_dir(file, mod_rs), *inline)
            if path is not None:
                path = path.replace("\\", "/")
                start = base if inline else posixpath.dirname(file)
                candidates = [(join(start, path), True)]
            else:
                candidates = [(join(base, name + ".rs"), False),
                              (join(base, name, "mod.rs"), True)]
            for candidate, candidate_mod_rs in candidates:
                if candidate in files:
                    todo.append((candidate, candidate_mod_rs))
                    break
    return reached


def manifest(text):
    """From a Cargo.toml: whether it has [package], the paths its targets
    name, the test targets' paths, and whether Cargo finds the tests
    itself (autotests)."""
    section = None
    is_package = False
    paths = []
    tests = []
    autotests = True
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        header = re.fullmatch(r"\[\[?([^\[\]]+)\]\]?", line)
        if header:
            section = header.group(1).strip()
            if section == "package":
                is_package = True
            continue
        setting = re.fullmatch(r"([A-Za-z0-9_-]+)\s*=\s*(.+)", line)
        if not setting:
            continue
        key, value = setting.group(1), setting.group(2).strip()
        text_value = re.fullmatch(r'"([^"]*)"', value)
        if section in ("lib", "bin", "test", "bench", "example") and key == "path" and text_value:
            paths.append(text_value.group(1))
            if section == "test":
                tests.append(text_value.group(1))
        elif section == "package" and key == "autotests" and value == "false":
            autotests = False
        elif section == "package" and key == "build" and text_value:
            paths.append(text_value.group(1))
    return is_package, paths, tests, autotests


def check_package(files):
    """The problems of one package, given as {relative path: text} with its
    Cargo.toml and every .rs file: a list of (path, message)."""
    is_package, paths, declared_tests, autotests = manifest(files.get("Cargo.toml", ""))
    if not is_package:
        return []
    roots = set(join(p.replace("\\", "/")) for p in paths)
    roots.update(["src/lib.rs", "src/main.rs", "build.rs"])
    problems = []
    for path in files:
        parts = path.split("/")
        if len(parts) == 3 and parts[0] in ("benches", "examples") and parts[2] == "main.rs":
            roots.add(path)
        if len(parts) == 4 and parts[:2] == ["src", "bin"] and parts[3] == "main.rs":
            roots.add(path)
        if len(parts) == 3 and parts[:2] == ["src", "bin"] and path.endswith(".rs"):
            roots.add(path)
        if len(parts) == 2 and parts[0] in ("benches", "examples") and path.endswith(".rs"):
            roots.add(path)
        if parts[0] == "tests" and autotests:
            if len(parts) == 3 and parts[2] == "main.rs":
                roots.add(path)
            elif len(parts) == 2 and path.endswith(".rs"):
                roots.add(path)
                if path not in declared_tests:
                    problems.append((path, "a test binary of its own: put it into a group folder "
                                     "tests/<group>/ and declare it in the group's main.rs "
                                     "(mod <file>;, mitcad#2496)"))
    reached = reach(files, sorted(roots))
    for path in sorted(files):
        if not path.endswith(".rs") or path in reached:
            continue
        if "tests" in path.split("/")[:-1]:
            problems.append((path, "not a module of any test binary: no mod declaration reaches it, "
                             "so it is never built or run (declare it in its group's main.rs, "
                             "or in the module of its folder)"))
    return sorted(problems)


def packages(folder):
    """Each package folder under a folder, with its files ({relative path:
    text}), not entering another package's folder."""
    found = []
    for top, dirs, names in os.walk(folder):
        dirs[:] = sorted(d for d in dirs if d not in SKIPPED and not d.startswith("."))
        if "Cargo.toml" not in names:
            continue
        with open(os.path.join(top, "Cargo.toml"), encoding="utf-8") as handle:
            if not manifest(handle.read())[0]:
                continue
        files = {}
        for inner, inner_dirs, inner_names in os.walk(top):
            if inner != top and "Cargo.toml" in inner_names:
                inner_dirs[:] = []
                continue
            inner_dirs[:] = sorted(d for d in inner_dirs if d not in SKIPPED and not d.startswith("."))
            for name in inner_names:
                if name.endswith(".rs") or (inner == top and name == "Cargo.toml"):
                    full = os.path.join(inner, name)
                    relative = os.path.relpath(full, top).replace(os.sep, "/")
                    with open(full, encoding="utf-8", errors="replace") as handle:
                        files[relative] = handle.read()
        found.append((top, files))
    return found


def scan(folders):
    problems = []
    counted = collections.Counter()
    seen = set()
    for folder in folders:
        for top, files in packages(folder):
            real = os.path.realpath(top)
            if real in seen:
                continue
            seen.add(real)
            counted["packages"] += 1
            counted["groups"] += sum(1 for p in files if re.fullmatch(r"tests/[^/]+/main\.rs", p))
            counted["test files"] += sum(1 for p in files
                                         if p.endswith(".rs") and "tests" in p.split("/")[:-1])
            for path, message in check_package(files):
                problems.append(f"{os.path.join(top, path)}: {message}")
    return problems, counted


def read_names(path):
    """A names file: {(package, kind, target): Counter of tests}."""
    tests = collections.defaultdict(collections.Counter)
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            line = line.rstrip("\n")
            if not line:
                continue
            package, target, test = line.split("\t")
            kind, _, name = target.partition(" ")
            tests[(package, kind, name)][test] += 1
    return tests


def compare(before, after):
    """The report of two builds' tests, and whether they are the same."""
    lines = []
    for title, tests in (("before", before), ("after", after)):
        kinds = collections.Counter(kind for (_, kind, _) in tests)
        listed = ", ".join(f"{kinds[k]} {k}" for k in sorted(kinds))
        lines.append(f"test binaries {title}: {sum(kinds.values())} ({listed})")
    left = collections.Counter()
    for (package, kind, target), names in after.items():
        for name, n in names.items():
            left[(package, kind, target, name)] += n
    by_name = collections.defaultdict(set)
    for (package, kind, target, name) in left:
        by_name[(package, kind, name)].add(target)
    missing = []
    total = 0
    for (package, kind, target), names in sorted(before.items()):
        for name, n in sorted(names.items()):
            for _ in range(n):
                total += 1
                key = None
                if kind == "test":
                    for group in sorted(by_name.get((package, kind, f"{target}::{name}"), ())):
                        if left[(package, kind, group, f"{target}::{name}")] > 0:
                            key = (package, kind, group, f"{target}::{name}")
                            break
                if key is None and left[(package, kind, target, name)] > 0:
                    key = (package, kind, target, name)
                if key is None:
                    missing.append(f"{package}\t{kind} {target}\t{name}")
                else:
                    left[key] -= 1
    new = sorted(f"{p}\t{k} {t}\t{n}" for (p, k, t, n), c in left.items() for _ in range(c))
    for line in missing:
        lines.append(f"missing after: {line}")
    for line in new:
        lines.append(f"new after: {line}")
    same = not missing and not new
    if same:
        lines.append(f"the same {total} tests before and after")
    return lines, same


def self_test():
    good = {
        "Cargo.toml": '[package]\nname = "planted"\n\n[dependencies]\nother = { path = "../other" }\n',
        "src/lib.rs": "#[cfg(test)]\nmod tests;\npub fn f() {}\n",
        "src/tests.rs": ('mod a_tests;\n#[cfg(unix)]\n#[path = "tests/renamed.rs"]\nmod b_tests;\n'
                         "// mod commented_tests;\n"),
        "src/tests/a_tests.rs": "#[test]\nfn a() {}\n",
        "src/tests/renamed.rs": "#[test]\nfn b() {}\n",
        "tests/grp/main.rs": ('//! A group.\nmod one;\n#[path = "../shared/helpers.rs"]\nmod helpers;\n'
                              "mod inline {\n    mod deep;\n}\n/* mod blocked; /* nested */ */\n"
                              'const S: &str = "mod quoted;";\nconst R: &str = r#"mod raw;"#;\n'
                              "fn f<'a>(x: &'a str) -> char { let _ = x; '}' }\n"),
        "tests/grp/one.rs": 'const DATA: &str = include_str!("../data/fixture.rs");\n#[test]\nfn t() {}\n',
        "tests/grp/inline/deep.rs": "#[test]\nfn d() {}\n",
        "tests/shared/helpers.rs": "pub fn h() {}\n",
        "tests/data/fixture.rs": "fn not_compiled() {}\n",
    }
    failures = []
    problems = check_package(good)
    if problems:
        failures.append(f"the planted good package has problems: {problems}")
    bad = dict(good)
    bad.update({
        "tests/top.rs": "#[test]\nfn top() {}\n",
        "tests/grp/orphan.rs": "#[test]\nfn o() {}\n",
        "tests/grp/commented.rs": "#[test]\nfn c() {}\n",
        "tests/grp/quoted.rs": "#[test]\nfn q() {}\n",
        "tests/grp/blocked.rs": "#[test]\nfn b() {}\n",
        "src/tests/commented_tests.rs": "#[test]\nfn c() {}\n",
        "src/tests/lost.rs": "#[test]\nfn l() {}\n",
    })
    expected = ["src/tests/commented_tests.rs", "src/tests/lost.rs", "tests/grp/blocked.rs",
                "tests/grp/commented.rs", "tests/grp/orphan.rs", "tests/grp/quoted.rs",
                "tests/top.rs"]
    reported = sorted(path for path, _ in check_package(bad))
    if reported != expected:
        failures.append(f"the planted bad package: reported {reported}, expected {expected}")
    declared = dict(good)
    declared["Cargo.toml"] += '\n[[test]]\nname = "custom"\npath = "tests/custom.rs"\nharness = false\n'
    declared["tests/custom.rs"] = "fn main() {}\n"
    if check_package(declared):
        failures.append(f"a declared test target is reported: {check_package(declared)}")

    def names(entries):
        tests = collections.defaultdict(collections.Counter)
        for package, target, test in entries:
            kind, _, name = target.partition(" ")
            tests[(package, kind, name)][test] += 1
        return tests

    before = names([("p", "lib p", "tests::a"), ("p", "test one", "t"), ("p", "test one", "m::u"),
                    ("p", "test two", "t"), ("p", "test kept", "k::x")])
    after = names([("p", "lib p", "tests::a"), ("p", "test grp", "one::t"),
                   ("p", "test grp", "one::m::u"), ("p", "test grp", "two::t"),
                   ("p", "test kept", "k::x")])
    lines, same = compare(before, after)
    if not same:
        failures.append(f"the planted lists differ: {lines}")
    after[("p", "test", "grp")]["two::t"] -= 1
    after[("p", "test", "grp")]["two::extra"] += 1
    lines, same = compare(before, after)
    if same or not any("missing after: p\ttest two\tt" in line for line in lines) \
            or not any("new after: p\ttest grp\ttwo::extra" in line for line in lines):
        failures.append(f"a lost and a new test are not reported: {lines}")
    for failure in failures:
        print(f"self-test: {failure}")
    if failures:
        return 1
    print("self-test: the planted problems are reported, and only those")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("folders", nargs="*")
    parser.add_argument("--compare", nargs=2, metavar=("BEFORE", "AFTER"))
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.compare:
        lines, same = compare(read_names(args.compare[0]), read_names(args.compare[1]))
        for line in lines:
            print(line)
        return 0 if same else 1
    if not args.folders:
        parser.error("no folders")
    problems, counted = scan(args.folders)
    for problem in problems:
        print(problem)
    if problems:
        print(f"{len(problems)} problems: see tools/rust-test-groups/check-rust-test-groups.py")
        return 1
    print(f"{counted['packages']} packages, {counted['groups']} grouped test binaries: "
          f"each of the {counted['test files']} test files is a module of a test binary")
    return 0


if __name__ == "__main__":
    sys.exit(main())
