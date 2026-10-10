#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Which ctests tools/check-all.sh runs: those whose sources changed.

Each test gets a fingerprint, a digest of the contents of what it depends
on, found from the build itself rather than from a list kept by hand:

- its own registration (command, properties; the checkout's and the build
  folder's paths left out, so that checkouts share the fingerprints);
- the files and folders of the source tree its command names (a cli
  test's JSON script, a cmake -P script, a design);
- for each executable its command names (the test program, mitcad-cli
  as -DCLI=..., ...), the CMake targets it is built from, through their
  dependencies (CMake's file API, codemodel-v2): each target's folder
  (every file under it, less the folders of other CMake directories and
  of Rust crates, and less the files that tests name: a cli test's JSON
  counts only for its test), its listed sources, and for a crate that
  Corrosion builds (cargo-build_<crate>) the crate's Rust closure; with
  them the build files (BUILD_FILES), the compiler and the build's
  options;
- for a crate's tests (core/cargo-tests.cmake run <package>, the fixture
  build, the doctests' cargo test --doc -p ...) the Rust closure of each
  package: the folders of the workspace crates it depends on (with dev-
  dependencies at the top), the files they include with include_str! or
  include_bytes!, the resolved external crates with their versions and
  features (cargo metadata) and RUST_FILES;
- for a script run by an interpreter (bash, python) the files beside it;
- for cargo-deny the workspace's manifests, its lock and deny.toml;
- the fingerprints of the fixtures the test requires.

A test whose command is none of these (an unknown program), or one in
WHOLE_TREE, depends on the whole tree; so does every test when the file
API's reply is missing (a build configured before check-all asked for it).
Markdown documents count for no test except through an include_str! (the
MCP server embeds commands.md).

  ctest-fingerprints.py plan --source DIR --build DIR --passed DIR
      [--mode auto|all] [--exclude REGEX] [--force NAME]... [--forced-only]
      [--explain NAME]...
    prints a line per test: name, run or skip, fingerprint, reason. A test
    runs when its fingerprint is not among those it passed with
    (<passed>/<name>), unless it is in FULL_ONLY (long and seldom
    affected: only with --mode all). Forced tests (named in check-all's
    --only) always run; with --forced-only no other test is listed.
  ctest-fingerprints.py record --plan FILE --log FILE --passed DIR
    keeps the fingerprint of each test the ctest log shows passed.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

# Tests that check the whole tree (every source file's licence header).
WHOLE_TREE = {"repo.spdx"}
# Tests that run only in the full check (--mode all, check-all --full):
# long, and seldom what a change breaks.
FULL_ONLY = {"app.appimage", "core.doctests_release"}
# What every C++ build depends on beyond its targets' folders.
BUILD_FILES = ["CMakeLists.txt", "CMakePresets.json", "cmake", "triplets", "vcpkg.json",
               "third_party", "rust-toolchain.toml"]
# What every Rust build depends on beyond its crates.
RUST_FILES = ["rust-toolchain.toml", "core/Cargo.toml", "core/.cargo"]
INTERPRETERS = re.compile(r"^(bash|sh|python[0-9.]*)(\.exe)?$")
INCLUDE = re.compile(rb'include_(?:str|bytes)!\s*\(\s*"([^"]+)"')
OPTIONS = re.compile(r"^(MITCAD_[A-Z0-9_]+:BOOL|CMAKE_BUILD_TYPE:STRING)=")


def md5(data):
    return hashlib.md5(data).hexdigest()


class Tree:
    """The source tree's files and their digests, by path relative to it."""

    def __init__(self, source, build):
        self.source = os.path.realpath(source)
        self.build = os.path.realpath(build)
        self.digests = {}
        # Folders that are not walked into: other CMake directories and
        # Rust crates (each has a fingerprint of its own).
        self.cmake_dirs = set()
        self.crate_dirs = set()
        # Files that tests name, which count only for those tests.
        self.test_files = set()

    def rel(self, path):
        """path relative to the source tree, or None outside it (or in the
        build folder)."""
        path = os.path.realpath(path)
        if path == self.build or path.startswith(self.build + os.sep):
            return None
        if path == self.source:
            return "."
        if path.startswith(self.source + os.sep):
            return os.path.relpath(path, self.source)
        return None

    def abs(self, rel):
        return os.path.join(self.source, rel)

    def digest(self, rel):
        if rel not in self.digests:
            try:
                with open(self.abs(rel), "rb") as f:
                    self.digests[rel] = md5(f.read())
            except OSError:
                self.digests[rel] = "missing"
        return self.digests[rel]

    def skipped_dir(self, rel, top):
        name = os.path.basename(rel)
        if name.startswith(".") and rel != top or name == "__pycache__":
            return True
        if os.path.realpath(self.abs(rel)) == self.build or rel == "build":
            return True
        if rel == top:
            return False
        if name == "target" and os.path.exists(os.path.join(self.abs(rel), "..", "Cargo.toml")):
            return True
        return False

    def files(self, rel, *, nested=True, flat=False, docs=False, test_files=True):
        """The files under rel (a file: itself), sorted. nested=False stops at
        other CMake directories and crates; flat: only rel's own files."""
        path = self.abs(rel)
        if os.path.isfile(path):
            return [rel]
        if not os.path.isdir(path):
            return []
        found = []
        for current, dirs, names in os.walk(path):
            crel = os.path.relpath(current, self.source)
            keep = []
            for d in dirs:
                drel = os.path.normpath(os.path.join(crel, d))
                if flat or self.skipped_dir(drel, rel):
                    continue
                if not nested and (drel in self.cmake_dirs or drel in self.crate_dirs or
                                   os.path.exists(os.path.join(current, d, "Cargo.toml"))):
                    continue
                keep.append(d)
            dirs[:] = keep
            for n in names:
                frel = os.path.normpath(os.path.join(crel, n))
                if not docs and n.endswith(".md"):
                    continue
                if not test_files and frel in self.test_files:
                    continue
                found.append(frel)
        return sorted(found)

    def lines(self, rels):
        return [f"{r} {self.digest(r)}" for r in rels]


class Cargo:
    """The workspace's crates and their resolved dependencies."""

    def __init__(self, tree):
        self.tree = tree
        self.ok = False
        self.memo = {}
        manifest = tree.abs("core/Cargo.toml")
        try:
            out = subprocess.run(["cargo", "metadata", "--offline", "--locked", "--format-version", "1",
                                  "--manifest-path", manifest],
                                 capture_output=True, check=True, timeout=300).stdout
            meta = json.loads(out)
        except (OSError, subprocess.SubprocessError, ValueError):
            return
        self.packages = {p["id"]: p for p in meta["packages"]}
        self.nodes = {n["id"]: n for n in (meta.get("resolve") or {}).get("nodes", [])}
        self.by_name = {}
        self.by_lib = {}
        for pid in meta["workspace_members"]:
            p = self.packages[pid]
            self.by_name[p["name"]] = pid
            # Corrosion names its targets after the crate's library or
            # program (cargo-build_mitcad_ffi, cargo-build_mitcad-release).
            for t in p["targets"]:
                if any(k in ("lib", "staticlib", "cdylib", "rlib", "bin") for k in t["kind"]):
                    self.by_lib[t["name"].replace("-", "_")] = pid
        for p in self.packages.values():
            if p["source"] is None:
                rel = tree.rel(os.path.dirname(p["manifest_path"]))
                if rel:
                    tree.crate_dirs.add(rel)
        self.ok = True

    def crate_lines(self, rel):
        """A workspace crate's files and what its sources include."""
        lines = []
        for f in self.tree.files(rel, nested=False):
            lines.append(f"{f} {self.tree.digest(f)}")
            if f.endswith(".rs"):
                try:
                    with open(self.tree.abs(f), "rb") as handle:
                        text = handle.read()
                except OSError:
                    continue
                for m in INCLUDE.finditer(text):
                    target = os.path.join(os.path.dirname(self.tree.abs(f)), m.group(1).decode())
                    trel = self.tree.rel(target)
                    if trel:
                        lines.append(f"{f} includes {trel} {self.tree.digest(trel)}")
        return lines

    def closure(self, name, dev):
        """The lines of a package's Rust closure (dev: with its
        dev-dependencies), or None for an unknown package."""
        key = (name, dev)
        if key in self.memo:
            return self.memo[key]
        if not self.ok:
            # Without cargo's resolution: the whole workspace and its lock.
            lines = self.tree.lines(self.tree.files("core") + ["core/Cargo.lock"])
            self.memo[key] = lines
            return lines
        pid = self.by_name.get(name) or self.by_lib.get(name.replace("-", "_"))
        if pid is None:
            return None
        lines = []
        seen = set()
        stack = [(pid, True)]
        while stack:
            current, top = stack.pop()
            if current in seen:
                continue
            seen.add(current)
            p = self.packages[current]
            node = self.nodes.get(current, {})
            features = ",".join(sorted(node.get("features", [])))
            if p["source"] is None:
                rel = self.tree.rel(os.path.dirname(p["manifest_path"]))
                lines.append(f"crate {p['name']} {features}")
                lines += self.crate_lines(rel) if rel else []
            else:
                lines.append(f"external {p['name']} {p['version']} {p['source']} {features}")
            for dep in node.get("deps", []):
                kinds = {k.get("kind") for k in dep.get("dep_kinds", [])}
                if (top and dev) or None in kinds or "build" in kinds:
                    stack.append((dep["pkg"], False))
        lines += self.tree.lines(f for r in RUST_FILES for f in self.tree.files(r))
        self.memo[key] = sorted(lines)
        return self.memo[key]


class CMake:
    """The build's targets (CMake's file API) and options."""

    def __init__(self, tree, build):
        self.tree = tree
        self.ok = False
        self.targets = {}
        self.artifacts = {}
        self.toolchain = []
        self.options = []
        self.memo = {}
        reply = os.path.join(build, ".cmake", "api", "v1", "reply")
        try:
            index_name = max(n for n in os.listdir(reply) if n.startswith("index-"))
            index = self.load(reply, index_name)
            codemodel = toolchains = None
            for obj in index["objects"]:
                if obj["kind"] == "codemodel":
                    codemodel = self.load(reply, obj["jsonFile"])
                elif obj["kind"] == "toolchains":
                    toolchains = self.load(reply, obj["jsonFile"])
            config = codemodel["configurations"][0]
        except (OSError, ValueError, KeyError, TypeError, IndexError):
            return
        dirs = [d["source"] for d in config["directories"]]
        for d in dirs:
            tree.cmake_dirs.add(os.path.normpath(d))
        for ref in config["targets"]:
            t = self.load(reply, ref["jsonFile"])
            t["dir"] = os.path.normpath(dirs[ref["directoryIndex"]])
            self.targets[t["id"]] = t
            for a in t.get("artifacts", []):
                path = a["path"] if os.path.isabs(a["path"]) else os.path.join(build, a["path"])
                self.artifacts[os.path.realpath(path)] = t["id"]
        for tc in (toolchains or {}).get("toolchains", []):
            c = tc.get("compiler", {})
            self.toolchain.append(f"toolchain {tc.get('language')} {c.get('id')} {c.get('version')}")
        try:
            with open(os.path.join(build, "CMakeCache.txt"), encoding="utf-8", errors="replace") as f:
                self.options = sorted("option " + line.strip() for line in f if OPTIONS.match(line))
        except OSError:
            pass
        self.ok = True

    @staticmethod
    def load(reply, name):
        with open(os.path.join(reply, name), encoding="utf-8") as f:
            return json.load(f)

    def closure(self, tid):
        seen = []
        stack = [tid]
        while stack:
            current = stack.pop()
            if current in seen or current not in self.targets:
                continue
            seen.append(current)
            stack += [d["id"] for d in self.targets[current].get("dependencies", [])]
        return seen

    def dir_lines(self, rel):
        if ("dir", rel) not in self.memo:
            files = (self.tree.files(f) for f in BUILD_FILES) if rel == "." else [
                self.tree.files(rel, nested=False, test_files=False)]
            self.memo[("dir", rel)] = self.tree.lines(f for group in files for f in group)
        return self.memo[("dir", rel)]

    def target_lines(self, tid, cargo):
        """The lines of what a target is built from, through its
        dependencies."""
        if tid in self.memo:
            return self.memo[tid]
        lines = set(self.toolchain) | set(self.options)
        for current in self.closure(tid):
            t = self.targets[current]
            lines.add(f"target {t['name']} {t['type']} {t['dir']}")
            lines.update(self.dir_lines(t["dir"]))
            for s in t.get("sources", []):
                if s.get("isGenerated"):
                    continue
                rel = self.tree.rel(s["path"] if os.path.isabs(s["path"]) else self.tree.abs(s["path"]))
                if rel:
                    lines.add(f"{rel} {self.tree.digest(rel)}")
            m = re.match(r"^_?cargo-build_(.+)$", t["name"])
            if m:
                crate = cargo.closure(m.group(1), dev=False)
                lines.update(crate if crate is not None else ["unknown crate " + m.group(1)])
        for f in BUILD_FILES:
            lines.update(self.tree.lines(self.tree.files(f)))
        self.memo[tid] = sorted(lines)
        return self.memo[tid]


def properties(test):
    return {p["name"]: p["value"] for p in test.get("properties", [])}


def test_strings(test):
    """The command's words and the environment's values."""
    words = list(test.get("command", []))
    env = properties(test).get("ENVIRONMENT", [])
    words += [e.split("=", 1)[1] for e in env if "=" in e]
    return words


def path_value(word):
    m = re.match(r"^-D[A-Za-z0-9_]+(?::[A-Z]+)?=(.*)$", word)
    return m.group(1) if m else word


def cargo_packages(command):
    """(packages, dev) a crate test runs, or None for another command."""
    names = [os.path.basename(w) for w in command]
    if "cargo-tests.cmake" in names and "--" in command:
        rest = command[command.index("--") + 1:]
        if rest and rest[0] in ("run", "parts") and len(rest) > 1:
            return [rest[1]]
        if rest and rest[0] == "build":
            return [w for w in rest[1:] if not w.startswith("-")]
    if command and re.match(r"^cargo(\.exe)?$", names[0]) and "--doc" in command:
        return [command[i + 1] for i, w in enumerate(command[:-1]) if w in ("-p", "--package")]
    return None


def normalized(test, tree):
    """The test's registration without the checkout's and build's paths."""
    text = json.dumps({"command": test.get("command", []), "properties": test.get("properties", [])},
                      sort_keys=True)
    return text.replace(tree.build, "<build>").replace(tree.source, "<source>")


def plan(args):
    tree = Tree(args.source, args.build)
    listing = subprocess.run(["ctest", "--test-dir", args.build, "--show-only=json-v1"],
                             capture_output=True, check=True).stdout
    tests = json.loads(listing)["tests"]
    exclude = re.compile(args.exclude) if args.exclude else None
    cmake = CMake(tree, args.build)
    cargo = Cargo(tree)
    # The files tests name.
    for test in tests:
        for word in test_strings(test):
            value = path_value(word)
            if os.path.isabs(value) and os.path.isfile(value):
                rel = tree.rel(value)
                if rel:
                    tree.test_files.add(rel)
    whole = None
    own = {}
    for test in tests:
        name = test["name"]
        lines = ["test " + normalized(test, tree)]
        command = test.get("command", [])
        known = False
        packages = cargo_packages(command)
        if packages is not None:
            known = True
            for package in packages:
                crate = cargo.closure(package, dev=True)
                lines += crate if crate is not None else ["unknown crate " + package]
        if name in WHOLE_TREE:
            known = False
        elif command and re.match(r"^cargo-deny(\.exe)?$", os.path.basename(command[0])):
            # The dependency policy: the manifests, the lock and the policy.
            known = True
            lines += tree.lines(f for f in tree.files("core")
                                if os.path.basename(f) in ("Cargo.toml", "Cargo.lock", "deny.toml"))
        elif command and os.path.realpath(command[0]) in cmake.artifacts:
            known = True
        elif command and re.match(r"^(cmake|ctest|cargo)(\.exe)?$", os.path.basename(command[0])):
            known = True
        interpreter = bool(command) and INTERPRETERS.match(os.path.basename(command[0]))
        if interpreter and name not in WHOLE_TREE:
            known = True
        for word in test_strings(test):
            value = path_value(word)
            if not os.path.isabs(value):
                continue
            real = os.path.realpath(value)
            if real in cmake.artifacts:
                lines += cmake.target_lines(cmake.artifacts[real], cargo)
                continue
            rel = tree.rel(real)
            if not rel or rel == ".":
                continue
            lines += tree.lines(tree.files(rel))
            if interpreter and real.endswith((".py", ".sh")):
                lines += tree.lines(tree.files(os.path.dirname(rel), flat=True))
        if not known or not cmake.ok:
            if whole is None:
                whole = tree.lines(tree.files("."))
            lines += whole
        own[name] = lines
    # The fixtures a test requires, through their setups. Not the fixtures
    # that build the crates' test binaries (cargo-tests.cmake build): a
    # crate's test depends on its own closure, not on every crate's.
    setups = {}
    for test in tests:
        command = test.get("command", [])
        if (cargo_packages(command) is not None and "--" in command and
                command[command.index("--") + 1:][:1] == ["build"]):
            continue
        for fixture in properties(test).get("FIXTURES_SETUP", []):
            setups.setdefault(fixture, []).append(test["name"])
    required = {t["name"]: properties(t).get("FIXTURES_REQUIRED", []) for t in tests}

    def with_fixtures(name, seen):
        lines = list(own[name])
        for fixture in required.get(name, []):
            for setup in setups.get(fixture, []):
                if setup not in seen and setup != name:
                    seen.add(setup)
                    lines += [f"fixture {fixture} {setup} {md5(chr(10).join(with_fixtures(setup, seen)).encode())}"]
        return sorted(set(lines))

    for name in args.explain or []:
        if name in own:
            sys.stderr.write(f"== {name}\n" + "".join(x + "\n" for x in with_fixtures(name, {name})))
    forced = set(args.force or [])
    out = []
    for test in tests:
        name = test["name"]
        if exclude and exclude.search(name):
            continue
        fingerprint = md5("\n".join(with_fixtures(name, {name})).encode())
        if name in forced:
            action, reason = "run", "named"
        elif args.forced_only:
            continue
        elif args.mode == "all":
            action, reason = "run", "all"
        elif name in FULL_ONLY:
            action, reason = "skip", "full check only"
        else:
            passed = set()
            try:
                with open(os.path.join(args.passed, name), encoding="utf-8") as f:
                    passed = {line.strip() for line in f}
            except OSError:
                pass
            if fingerprint in passed:
                action, reason = "skip", "passed with these sources"
            else:
                action, reason = "run", "changed"
        out.append(f"{name}\t{action}\t{fingerprint}\t{reason}")
    sys.stdout.write("".join(line + "\n" for line in out))
    return 0


def record(args):
    passed = set()
    line = re.compile(r"^\s*\d+/\d+ +Test +#\d+: +(\S+) [ .]*Passed +[0-9.]+ sec$")
    with open(args.log, encoding="utf-8", errors="replace") as f:
        for text in f:
            m = line.match(text.rstrip("\n"))
            if m:
                passed.add(m.group(1))
    os.makedirs(args.passed, exist_ok=True)
    kept = 0
    with open(args.plan, encoding="utf-8") as f:
        for text in f:
            fields = text.rstrip("\n").split("\t")
            if len(fields) < 3 or fields[0] not in passed:
                continue
            path = os.path.join(args.passed, fields[0])
            try:
                with open(path, encoding="utf-8") as old:
                    if fields[2] in {x.strip() for x in old}:
                        continue
            except OSError:
                pass
            with open(path, "a", encoding="utf-8") as new:
                new.write(fields[2] + "\n")
            kept += 1
    print(f"{len(passed)} tests passed, {kept} new fingerprints kept")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="action", required=True)
    p = sub.add_parser("plan")
    p.add_argument("--source", required=True)
    p.add_argument("--build", required=True)
    p.add_argument("--passed", required=True)
    p.add_argument("--mode", choices=("auto", "all"), default="auto")
    p.add_argument("--exclude")
    p.add_argument("--force", action="append")
    p.add_argument("--forced-only", action="store_true")
    p.add_argument("--explain", action="append", help="print what a test's fingerprint is made of")
    r = sub.add_parser("record")
    r.add_argument("--plan", required=True)
    r.add_argument("--log", required=True)
    r.add_argument("--passed", required=True)
    args = parser.parse_args()
    return plan(args) if args.action == "plan" else record(args)


if __name__ == "__main__":
    sys.exit(main())
