#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Before/after proof of the OCCT port patches' regression tests (mitcad#1986).

Each patch in third_party/vcpkg-ports/opencascade/ has its regression test
named in its section of third_party/vcpkg-ports/docs/opencascade-patch-tests.md
(`## <number>`, then a `Test:` paragraph naming ctests such as
geometry.port.0011 or geometry.suite.fillet_overflow); the patch's header
only points there, so that a test note or a proof result does not change
the port and rebuild OCCT (mitcad#2133). This tool shows, for each patch
given, that the named ctests pass with the whole port and fail with that
one patch reverted:

  1. OCCT 8.0.1 with every port patch as one commit each, in portfile order
     (tags p-<number>, tip), in <work>/history.
  2. The port's tip built (modelling modules, Release, shared) in <work>/tip;
     the named ctests run "with" from the MitCad build <repo>/build/<preset>
     against those libraries (LD_LIBRARY_PATH).
  3. Workers, each with its own OCCT tree (<work>/w<i>) and its own MitCad
     build (<repo>/build/occt-proof-m<i>), take the patches one at a time:
     revert the patch at the tip (git revert), rebuild OCCT incrementally,
     copy the OCCT headers that differ into the MitCad build's installed
     headers and rebuild the tests that include them, and run the named
     ctests "without" against the reverted libraries.
  4. A Markdown table: patch, test, with, without, verdict.

A patch that does not revert alone at the tip (a later patch changed the
same lines) is reverted together with the later patches that last changed
the conflicting lines (git blame of each conflict), as few as make the
revert apply, at most --max-extra; the table names them. Where OCCT then
does not build, an #include the reverted patch added is kept, and a later
patch using a function or variable it added is reverted too. A group can
also be given as 0130+0140.

A failing ctest counts for the patch when the failure lies in a test
function the patch's Test paragraph names (test_...); where the output does not
place the failure, each named function runs alone (MITCAD_TEST_ONLY).

    tools/occt-port-proof.py [options] NNNN [NNNN+MMMM ...]
    tools/occt-port-proof.py --unproven        # sections that say "still to be shown"
    tools/occt-port-proof.py --all             # every patch naming a ctest
    tools/occt-port-proof.py --list            # the patches and their named ctests only
    tools/occt-port-proof.py --update-tests [NNNN ...]
                                               # rewrite the proof sentences from the last
                                               # results (all, or the patches given)

--update-tests (also spelled --update-headers, its name before mitcad#2133)
writes the test document and the table, never a .patch file. --tests-doc
reads and writes another copy of the test document. A patch header that has
a `Test:` paragraph again is reported: its test belongs in the document.

In the chain (--in-chain, mitcad#2121) a patch is shown at its own place
instead of at the tip: OCCT with the patches before it (*before*) and with
the patches up to it (*with it*), the MitCad tests built against each
state's headers and its named ctests run against each state's libraries.
No later patch is reverted, so a patch entangled with later ones at the
tip is shown here alone. Where the MitCad code the tests need (the
geometry library, the test's own source) uses a later patch's API (a
function, a member, a header it adds), that patch's declarations (its
changes to the headers alone) are applied to both states (*carried*, at
most --max-carry, 20); a function only declared there is bound, in the
test's executable, to a stub that says so if it is called (the patch
defining it is then carried whole, where it applies there; else the run
shows nothing). Where the declarations do not apply or OCCT does not build
with them, the patch is cherry-picked whole, with the patches between
whose lines its hunks build on where it does not cherry-pick alone;
where the patch itself adds the API, the test does not build without the
patch. A suite part whose
executable has other parts that do not build at that state is linked
without them. The --jobs workers (trees <work>/c<i>, MitCad builds
<repo>/build/occt-chain-m<i>) each take a run of consecutive patches and
walk it forwards, so that each step rebuilds only what the next patch
changes. Results (chain-results.jsonl) and every state's build and test
runs (chain-states.jsonl, chain-runs.jsonl) are kept: an interrupted run
started again goes on where it stopped (--chain-retry runs the given
patches again).

    tools/occt-port-proof.py --in-chain --jobs 5 NNNN [...]
    tools/occt-port-proof.py --in-chain --table-verdict "not shown" --table-verdict "fails with"

(the second takes the patches whose verdict at the tip in the table starts
so). The table gets three columns for the chain, and --update-tests
writes the chain's result into the test document's sections of the
patches the tip did not prove.

Run it in a login shell (corpus variables) with the MitCad build of the
preset configured (cmake --preset dev); --list without that build lists
every ctest name a paragraph mentions, unchecked. Heavy steps run through --wrap, e.g.
--wrap "bash /path/memjob.sh 20" to give each worker its own memory-capped
scope. Nothing is deleted: work trees are reused on the next run. The
table keeps the rows of an earlier table whose results are no longer in
the work folder.
"""

from __future__ import annotations

import argparse
import datetime
import fcntl
import hashlib
import json
import os
import re
import shlex
import signal
import subprocess
import sys
import textwrap
import time
import traceback
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PORT = REPO / "third_party" / "vcpkg-ports" / "opencascade"
TARBALL = "Open-Cascade-SAS-OCCT-V8_0_1.tar.gz"
TOOL = "tools/occt-port-proof.py"
# The ctest prefixes a Test paragraph may name.
CTEST_NAME = re.compile(r"(?<![\w/.-])((?:geometry|cli|core|app)\.[A-Za-z0-9_.-]*[A-Za-z0-9_])")
OCCT_CMAKE = [
    "-G", "Ninja", "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_LIBRARY_TYPE=Shared",
    "-DBUILD_MODULE_ApplicationFramework=OFF", "-DBUILD_MODULE_DataExchange=OFF",
    "-DBUILD_MODULE_Draw=OFF", "-DBUILD_MODULE_Visualization=OFF", "-DBUILD_DOC_Overview=OFF",
    "-DBUILD_RESOURCES=OFF", "-DUSE_TK=OFF", "-DUSE_FREETYPE=OFF", "-DUSE_OPENGL=OFF",
    "-DUSE_XLIB=OFF", "-DUSE_TBB=OFF", "-DBUILD_USE_PCH=OFF", "-DBUILD_GTEST=OFF",
    "-DCMAKE_CXX_COMPILER_LAUNCHER=ccache", "-DCMAKE_C_COMPILER_LAUNCHER=ccache",
]


def log(msg: str) -> None:
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def run(cmd, cwd=None, env=None, check=True, out=None) -> subprocess.CompletedProcess:
    """Runs a command; its output goes to the file `out` (appended) or is captured."""
    if out is not None:
        with open(out, "a", encoding="utf-8", errors="replace") as f:
            f.write(f"\n$ {shlex.join(str(c) for c in cmd)}\n")
            f.flush()
            p = subprocess.run([str(c) for c in cmd], cwd=cwd, env=env, stdout=f, stderr=subprocess.STDOUT)
    else:
        p = subprocess.run([str(c) for c in cmd], cwd=cwd, env=env, capture_output=True, text=True)
    if check and p.returncode != 0:
        detail = "" if out is not None else (p.stderr or p.stdout)[-2000:]
        raise RuntimeError(f"failed ({p.returncode}): {shlex.join(str(c) for c in cmd)}\n{detail}")
    return p


def git(repo: Path, *args, check=True) -> subprocess.CompletedProcess:
    return run(["git", "-C", repo, *args], check=check)


# ---------------------------------------------------------------- the port


def patch_order() -> list[str]:
    """The patch files in portfile.cmake's PATCHES order."""
    text = (PORT / "portfile.cmake").read_text()
    block = re.search(r"PATCHES\s+(.*?)\)", text, re.S)
    if not block:
        raise SystemExit("portfile.cmake: no PATCHES list")
    return re.findall(r"[\w.-]+\.patch", block.group(1))


def number(patch_file: str) -> str:
    return patch_file.split("-", 1)[0]


def header(patch_file: str) -> str:
    text = (PORT / patch_file).read_text(encoding="utf-8", errors="replace")
    m = re.search(r"^(diff --git |--- a/|Index: )", text, re.M)
    return text[: m.start()] if m else text


def test_paragraphs(text: str) -> list[tuple[int, int]]:
    """The spans of the text's Test paragraphs (from 'Test' to a blank line)."""
    spans = []
    for m in re.finditer(r"^Tests?:", text, re.M):
        end = text.find("\n\n", m.start())
        spans.append((m.start(), len(text) if end < 0 else end))
    return spans


# ---------------------------------------------------------------- the test document

# The patches' tests and proof sentences live outside the port folder, whose
# every file vcpkg hashes (mitcad#2133): one section per patch, headed
# "## <number>", holding its Test paragraph.
TESTS_DOC = PORT.parent / "docs" / "opencascade-patch-tests.md"
SECTION = re.compile(r"^## (\d{4})[ \t]*\n(.*?)(?=^## |\Z)", re.M | re.S)
_doc: dict = {}


def tests_doc() -> Path:
    return Path(_doc.get("path", TESTS_DOC))


def set_tests_doc(path: str) -> None:
    _doc.clear()
    _doc["path"] = path


def test_sections() -> dict[str, str]:
    """Patch number -> the text of its section in the test document."""
    if "sections" not in _doc:
        p = tests_doc()
        text = p.read_text(encoding="utf-8") if p.exists() else ""
        _doc["sections"] = {m.group(1): m.group(2) for m in SECTION.finditer(text)}
    return _doc["sections"]


def tests_of(n: str) -> str:
    """The section of patch `n` in the test document ('' without one)."""
    return test_sections().get(n, "")


def stray_test_paragraphs(order: list[str]) -> list[str]:
    """The patches whose header has a Test paragraph (it belongs in the document)."""
    return [number(f) for f in order if test_paragraphs(header(f))]


def named_ctests(text: str, known: set[str] | None) -> list[str]:
    """The ctests a section's Test paragraphs name, in order, known names only
    (every name with `known` None).

    '(ctests geometry.a.b, c and d)' names geometry.a.b, geometry.a.c and
    geometry.a.d."""
    names: list[str] = []
    for start, end in test_paragraphs(text):
        para = re.sub(r"\s+", " ", text[start:end])
        found = [m.group(1) for m in CTEST_NAME.finditer(para)]
        for group in re.findall(r"\(ctests?\s+([^)]*)\)", para):
            prefix = ""
            for item in re.split(r",|\band\b", group):
                item = item.strip().split(" ")[0] if item.strip() else ""
                if not item:
                    continue
                if "." in item:
                    prefix = item.rsplit(".", 1)[0] + "."
                elif prefix:
                    found.append(prefix + item)
        for name in found:
            name = name.rstrip(".")
            if (known is None or name in known) and name not in names:
                names.append(name)
    return names


def unproven(text: str) -> bool:
    return re.search(r"still\s+to\s+be\s+shown", text) is not None


# ---------------------------------------------------------------- OCCT trees


def find_tarball(given: str | None) -> Path:
    places = [given] if given else []
    for var in ("VCPKG_DOWNLOADS",):
        if os.environ.get(var):
            places.append(os.path.join(os.environ[var], TARBALL))
    if os.environ.get("VCPKG_ROOT"):
        places.append(os.path.join(os.environ["VCPKG_ROOT"], "downloads", TARBALL))
    places.append(os.path.expanduser(f"~/vcpkg/downloads/{TARBALL}"))
    for p in places:
        if p and Path(p).is_file():
            return Path(p)
    raise SystemExit(f"{TARBALL} not found (--tarball, VCPKG_DOWNLOADS, VCPKG_ROOT/downloads)")


def port_stamp(order: list[str]) -> str:
    """The port's code: the patches' bodies, not their headers, so that an
    edited header keeps the history and the with-runs (mitcad#2121: a header
    rewrite between two runs dropped the earlier with-runs, and their
    patches' rows read "does not pass with the patch"; the proof sentences
    are written to the test document since mitcad#2133)."""
    h = hashlib.sha256()
    for f in order:
        h.update(f.encode())
        h.update((PORT / f).read_text(encoding="utf-8", errors="replace")[len(header(f)):].encode())
    return h.hexdigest()[:16]


def make_history(work: Path, tarball: Path, order: list[str]) -> Path:
    """OCCT 8.0.1 plus each patch as a commit (tags base, p-<number>, tip)."""
    hist = work / "history"
    stamp = port_stamp(order)
    stamp_file = work / "history.stamp"
    if stamp_file.exists() and stamp_file.read_text().strip() == stamp:
        return hist
    log(f"history: OCCT 8.0.1 + {len(order)} patches")
    if not (hist / ".git").exists():
        hist.mkdir(parents=True, exist_ok=True)
        run(["tar", "-xzf", tarball, "-C", hist, "--strip-components=1"])
        git(hist, "init", "-q")
        for k, v in (("user.name", "port"), ("user.email", "port@localhost"), ("core.autocrlf", "false")):
            git(hist, "config", k, v)
        git(hist, "add", "-A")
        git(hist, "commit", "-q", "-m", "OCCT 8.0.1")
        git(hist, "tag", "-f", "base")
    else:
        git(hist, "checkout", "-q", "-f", "--detach", "base")
    for f in order:
        p = git(hist, "apply", "--ignore-whitespace", "--whitespace=nowarn", PORT / f, check=False)
        if p.returncode != 0:
            raise SystemExit(f"history: {f} does not apply:\n{p.stderr}")
        git(hist, "add", "-A")
        git(hist, "commit", "-q", "--allow-empty", "-m", f)
        git(hist, "tag", "-f", f"p-{number(f)}")
    git(hist, "tag", "-f", "tip")
    stamp_file.write_text(stamp + "\n")
    return hist


def sync_tree(tree: Path, hist: Path) -> None:
    """A clone of the history at its tip (fetched again when the history changed)."""
    if not (tree / ".git").exists():
        run(["git", "clone", "-q", "--no-checkout", hist, tree])
    # No automatic gc in the background, whose locks would stop the reverts.
    git(tree, "config", "gc.auto", "0")
    git(tree, "fetch", "-q", "--force", "--tags", hist)
    git(tree, "checkout", "-q", "-f", "--detach", "tip")


def ccache_env(work: Path) -> dict:
    env = dict(os.environ)
    # The trees are <work>/<name>/src and .../build: with the base directory
    # their objects share ccache entries.
    env["CCACHE_BASEDIR"] = str(work)
    env["CCACHE_NOHASHDIR"] = "1"
    env.pop("DISPLAY", None)
    return env


def build_occt(tree: Path, work: Path, jobs: int, out: Path) -> bool:
    bld = tree.parent / "build"
    env = ccache_env(work)
    # OCCT reads its source lists (FILES.cmake) as data, so CMake does not
    # notice a source file a reverted patch removes or adds: configure again
    # whenever the tracked files or those lists differ from the last time.
    listing = git(tree, "ls-files", "-s", "--", ":(glob)**/FILES.cmake").stdout + git(tree, "ls-files").stdout
    stamp = hashlib.sha256(listing.encode()).hexdigest()
    stamp_file = bld / "occt-port-proof.stamp"
    if not (bld / "build.ninja").exists() or not stamp_file.exists() or stamp_file.read_text() != stamp:
        run(["cmake", "-S", tree, "-B", bld, *OCCT_CMAKE], env=env, out=out)
        stamp_file.write_text(stamp)
    start = out.stat().st_size if out.exists() else 0
    ok = run(["ninja", "-C", bld, "-j", str(jobs)], env=env, out=out, check=False).returncode == 0
    if not ok:
        with open(out, "rb") as f:
            f.seek(start)
            last_occt_errors[0] = f.read().decode("utf-8", "replace")
    return ok


last_occt_errors = [""]


def occt_libs(tree: Path) -> Path:
    bld = tree.parent / "build"
    for cand in (bld / "lin64" / "gcc" / "lib", bld / "lib"):
        if cand.is_dir():
            return cand
    raise RuntimeError(f"no library folder in {bld}")


# ---------------------------------------------------------------- MitCad builds


def ctest_table(build: Path) -> dict:
    p = run(["ctest", "--test-dir", build, "--show-only=json-v1"])
    return {t["name"]: t for t in json.loads(p.stdout)["tests"]}


def configure_mitcad(build: Path, preset: str, out: Path) -> None:
    if not (build / "build.ninja").exists():
        run(["cmake", "--preset", preset, "-B", build], cwd=REPO, out=out)


def build_mitcad(build: Path, targets: list[str], jobs: int, out: Path) -> bool:
    env = dict(os.environ)
    env["CCACHE_BASEDIR"] = str(REPO)
    env["CCACHE_NOHASHDIR"] = "1"
    start = out.stat().st_size if out.exists() else 0
    ok = run(["ninja", "-C", build, "-j", str(jobs), *targets], env=env, out=out, check=False).returncode == 0
    if not ok:
        with open(out, "rb") as f:
            f.seek(start)
            text = f.read().decode("utf-8", "replace")
        last_failed.clear()
        for m in re.finditer(r"^FAILED: (?:\[code=\d+\] )?(\S+)", text, re.M):
            last_failed.append(re.sub(r"^.*\.dir/(.*)\.o$", r"\1", m.group(1)))
    return ok


last_failed: list[str] = []


def installed_headers(build: Path) -> Path:
    for cand in sorted((build / "vcpkg_installed").glob("*/include/opencascade")):
        if (cand / "Standard.hxx").exists():
            return cand
    raise RuntimeError(f"no installed OCCT headers under {build}/vcpkg_installed")


def as_installed(text: bytes) -> bytes:
    """A header as the port installs it (portfile.cmake: <X.hxx> -> "X.hxx")."""
    return re.sub(rb'(# *include) <([a-zA-Z0-9_]*[.][hgl]xx)>', rb'\1 "\2"', text)


def port_headers(hist: Path) -> dict[str, str]:
    """The installed name -> source path of every header a port patch changes or adds."""
    p = git(hist, "diff", "--name-only", "base", "tip")
    out = {}
    for path in p.stdout.split():
        # .pxx files are private: the port does not install them.
        if path.startswith("src/") and re.search(r"\.(hxx|lxx|gxx|h)$", path):
            out[Path(path).name] = path
    return out


MISSING = b'#error "this header is not in this state of the port (occt-port-proof)"\n'


def sync_headers(tree: Path, inc: Path, headers: dict[str, str]) -> list[str]:
    """Writes into `inc` each port header whose content differs from the tree's."""
    changed = []
    for name, path in headers.items():
        src = tree / path
        want = as_installed(src.read_bytes()) if src.exists() else MISSING
        dst = inc / name
        have = dst.read_bytes() if dst.exists() else None
        if have is None and want == MISSING:
            continue
        if have != want:
            # A new file in place of the old: vcpkg installs these headers as
            # hard links into its shared packages folder, which writing the
            # file itself would change for every tree linked to it.
            tmp = dst.with_name(dst.name + ".occt-port-proof")
            tmp.write_bytes(want)
            os.replace(tmp, dst)
            changed.append(name)
    return changed


# ---------------------------------------------------------------- running a ctest


def apply_env_mods(env: dict, mods: list[str]) -> None:
    for mod in mods:
        var, _, rest = mod.partition("=")
        op, _, val = rest.partition(":")
        cur = env.get(var, "")
        if op == "set":
            env[var] = val
        elif op in ("unset", "reset"):
            env.pop(var, None)
        elif op == "string_append":
            env[var] = cur + val
        elif op == "string_prepend":
            env[var] = val + cur
        elif op == "path_list_append":
            env[var] = f"{cur}:{val}" if cur else val
        elif op == "path_list_prepend":
            env[var] = f"{val}:{cur}" if cur else val
        elif op == "cmake_list_append":
            env[var] = f"{cur};{val}" if cur else val
        elif op == "cmake_list_prepend":
            env[var] = f"{val};{cur}" if cur else val


def run_ctest(test: dict, libdir: Path, logfile: Path, timeout: float, extra_env: dict | None = None) -> dict:
    """Runs one ctest's command as ctest would, OCCT taken from libdir."""
    props = {p["name"]: p["value"] for p in test.get("properties", [])}
    env = dict(os.environ)
    env.update(extra_env or {})
    env.pop("DISPLAY", None)
    env.pop("WAYLAND_DISPLAY", None)
    for kv in props.get("ENVIRONMENT", []):
        k, _, v = kv.partition("=")
        env[k] = v
    apply_env_mods(env, props.get("ENVIRONMENT_MODIFICATION", []))
    env["LD_LIBRARY_PATH"] = f"{libdir}:{env['LD_LIBRARY_PATH']}" if env.get("LD_LIBRARY_PATH") else str(libdir)
    limit = min(timeout, float(props.get("TIMEOUT", 1500) or 1500))
    start = time.time()
    timed_out = False
    with open(logfile, "w", encoding="utf-8", errors="replace") as f:
        f.write(f"$ LD_LIBRARY_PATH={libdir} {shlex.join(test['command'])}\n")
        f.flush()
        proc = subprocess.Popen(test["command"], cwd=props.get("WORKING_DIRECTORY"), env=env,
                                stdout=f, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            rc = proc.wait(timeout=limit)
        except subprocess.TimeoutExpired:
            timed_out = True
            os.killpg(proc.pid, signal.SIGKILL)
            rc = proc.wait()
    secs = time.time() - start
    output = logfile.read_text(encoding="utf-8", errors="replace")
    if timed_out:
        status = "timeout"
    elif props.get("SKIP_RETURN_CODE") is not None and rc == int(props["SKIP_RETURN_CODE"]):
        status = "skipped"
    elif any(re.search(r, output) for r in props.get("SKIP_REGULAR_EXPRESSION", [])):
        status = "skipped"
    else:
        if props.get("PASS_REGULAR_EXPRESSION"):
            ok = any(re.search(r, output) for r in props["PASS_REGULAR_EXPRESSION"])
        else:
            ok = rc == 0
        if any(re.search(r, output) for r in props.get("FAIL_REGULAR_EXPRESSION", [])):
            ok = False
        if props.get("WILL_FAIL"):
            ok = not ok
        status = "passed" if ok else "failed"
    why = ""
    if status in ("failed", "timeout"):
        lines = [ln.strip() for ln in output.splitlines()[1:]
                 if re.search(r"FAIL|[Ff]ailed|error|Assertion|Segmentation|terminate|symbol lookup", ln)]
        why = "timeout" if timed_out else (re.sub(r"^(\.\./)+", "", lines[0])[:160] if lines else f"exit {rc}")
    return {"status": status, "seconds": round(secs, 1), "rc": rc, "why": why, "log": str(logfile)}


# ---------------------------------------------------------------- reverting


class Reverter:
    """Reverts patches at the tip of a tree cloned from the history."""

    def __init__(self, tree: Path, order: list[str]):
        self.tree = tree
        self.order = [number(f) for f in order]
        self.files: dict[str, set[str]] = {}
        self.added: dict[str, str] = {}
        log_lines = git(tree, "log", "--format=%H %s", "base..tip").stdout.splitlines()
        self.patch_of = {sha: number(subject) for sha, _, subject in (ln.partition(" ") for ln in log_lines)}

    def files_of(self, n: str) -> set[str]:
        if n not in self.files:
            p = git(self.tree, "diff", "--name-only", f"p-{n}~1", f"p-{n}")
            self.files[n] = set(p.stdout.split())
        return self.files[n]

    def added_text(self, n: str) -> str:
        """The lines patch n adds."""
        if n not in self.added:
            diff = git(self.tree, "diff", f"p-{n}~1", f"p-{n}").stdout
            self.added[n] = "\n".join(ln[1:] for ln in diff.splitlines() if ln.startswith("+") and not ln.startswith("+++"))
        return self.added[n]

    def desc(self, items) -> list[str]:
        return sorted(items, key=self.order.index, reverse=True)

    def reset(self) -> None:
        for attempt in range(4):
            p = git(self.tree, "checkout", "-q", "-f", "--detach", "tip", check=False)
            if p.returncode == 0 and git(self.tree, "reset", "-q", "--hard", "tip", check=False).returncode == 0:
                return
            log(f"{self.tree}: git: {(p.stderr or '').strip().splitlines()[:1]}")
            git(self.tree, "revert", "--quit", check=False)
            lock = self.tree / ".git" / "index.lock"
            if lock.exists() and attempt > 0:
                # A lock left by a git that was killed: moved aside, not deleted.
                lock.rename(self.tree.parent / f"index.lock.{int(time.time())}")
            time.sleep(2)
        raise RuntimeError(f"{self.tree}: cannot go back to the tip")

    def try_revert(self, seq: list[str]) -> tuple[bool, set[str], set[str]]:
        """Reverts seq in order; on a conflict, (False, the conflicting files,
        the later patches that last changed the conflicting lines)."""
        self.reset()
        for n in seq:
            if git(self.tree, "revert", "--no-commit", "--no-edit", f"p-{n}", check=False).returncode != 0:
                files = set(git(self.tree, "diff", "--name-only", "--diff-filter=U").stdout.split())
                culprits = set()
                for f in files:
                    culprits |= self.culprits(n, f)
                git(self.tree, "revert", "--abort", check=False)
                self.reset()
                return False, files, culprits
        return True, set(), set()

    def culprits(self, n: str, path: str) -> set[str]:
        """The patches after n that last changed the lines of a conflict in path."""
        ours = run(["git", "-C", self.tree, "show", f":2:{path}"], check=False).stdout
        if not ours:
            return set()
        tmp = self.tree.parent / "ours.tmp"
        tmp.write_text(ours, encoding="utf-8")
        blame = git(self.tree, "blame", "--porcelain", "--contents", tmp, "HEAD", "--", path, check=False).stdout
        owners: list[str] = []
        sha = ""
        for ln in blame.splitlines():
            m = re.match(r"^([0-9a-f]{40}) \d+ \d+", ln)
            if m:
                sha = m.group(1)
            elif ln.startswith("\t"):
                owners.append(self.patch_of.get(sha, ""))
        ours_lines = ours.split("\n")
        work = (self.tree / path).read_text(encoding="utf-8", errors="replace").split("\n")
        found: set[str] = set()
        i = 0
        while i < len(work):
            if not work[i].startswith("<<<<<<<"):
                i += 1
                continue
            j = i + 1
            block = []
            while j < len(work) and not work[j].startswith(("=======", "|||||||")):
                block.append(work[j])
                j += 1
            probe = block or work[max(0, i - 2):i]
            for k in range(len(ours_lines) - len(probe) + 1):
                if probe and ours_lines[k:k + len(probe)] == probe:
                    for o in owners[max(0, k - 1):k + len(probe) + 1]:
                        if o and o in self.order and self.order.index(o) > self.order.index(n):
                            found.add(o)
                    break
            while j < len(work) and not work[j].startswith(">>>>>>>"):
                j += 1
            i = j + 1
        return found

    def revert(self, group: list[str], cap: int = 40) -> list[str] | None:
        """Reverts the group at the tip, with the later patches that changed
        the same lines (found by blaming each conflict); returns the patches
        reverted (latest first), or None when more than `cap` later patches
        would have to go with it."""
        chosen = set(group)
        while True:
            if len(chosen) - len(group) > cap:
                self.reset()
                return None
            ok, files, culprits = self.try_revert(self.desc(chosen))
            if ok:
                break
            new = culprits - chosen
            if not new:
                # No owner found: the nearest later patch touching a conflicting file.
                first = min(self.order.index(n) for n in chosen)
                later = [n for n in self.order[first + 1:] if n not in chosen and self.files_of(n) & files]
                new = set(later[:1])
            if not new:
                self.reset()
                return None
            chosen |= new
        extras = [n for n in self.desc(chosen) if n not in group]
        if extras:
            for cand in extras:
                trial = chosen - {cand}
                if self.try_revert(self.desc(trial))[0]:
                    chosen = trial
        final = self.desc(chosen)
        if not self.try_revert(final)[0]:
            return None
        return final

    def keep_includes(self, reverted: list[str], log_text: str, bld: Path) -> list[list[str]]:
        """Adds back, to each file the compiler reports, the #include <X.hxx>
        a reverted patch added for a name X now undeclared there; returns
        the [file, line] pairs added."""
        added = "\n".join(self.added_text(n) for n in reverted)
        out = []
        pat = (r"^(\S+\.[ch]xx):\d+:\d+: error: (?:[‘']([A-Za-z_]\w*)[’'] (?:has not been declared|was not "
               r"declared|does not name a type|is not a member)|.*incomplete type [‘'](?:class |struct )?([A-Za-z_]\w*)[’'])")
        for m in re.finditer(pat, log_text, re.M):
            name = m.group(2) or m.group(3)
            path = (bld / m.group(1)).resolve()
            try:
                rel = str(path.relative_to(self.tree.resolve()))
            except ValueError:
                continue
            for hdr in (f"{name}.hxx",):
                line = f"#include <{hdr}>"
                if line in added and [rel, line] not in out:
                    out.append([rel, line])
        return [x for x in out if self.apply_includes([x])]

    def apply_includes(self, items: list[list[str]]) -> bool:
        """Inserts each include after the file's last #include; False when none was missing."""
        changed = False
        for rel, line in items:
            f = self.tree / rel
            lines = f.read_text(encoding="utf-8", errors="replace").split("\n")
            if line in lines:
                continue
            last = max((i for i, ln in enumerate(lines) if ln.startswith("#include")), default=-1)
            lines.insert(last + 1, line)
            f.write_text("\n".join(lines), encoding="utf-8")
            changed = True
        return changed

    def users(self, reverted: list[str], log_text: str) -> set[str]:
        """The later patches, not reverted, that use a name a reverted patch
        adds and the compiler reports missing."""
        names = set()
        for ln in log_text.splitlines():
            if "error:" in ln:
                for q in re.findall(r"[‘']([A-Za-z_][\w:]*)", ln):
                    names.add(q.split("::")[-1])
        added = "\n".join(self.added_text(n) for n in reverted)
        names = {x for x in names if re.search(rf"\b{re.escape(x)}\b", added)}
        first = min(self.order.index(n) for n in reverted)
        out = set()
        for n in self.order[first + 1:]:
            if n in reverted:
                continue
            text = self.added_text(n)
            if any(re.search(rf"\b{re.escape(x)}\b", text) for x in names):
                out.add(n)
        return out


# ---------------------------------------------------------------- the worker


def locked_pop(queue: Path) -> str | None:
    with open(queue.with_suffix(".lock"), "w") as lk:
        fcntl.flock(lk, fcntl.LOCK_EX)
        items = json.loads(queue.read_text())
        if not items:
            return None
        item = items.pop(0)
        queue.write_text(json.dumps(items))
        return item


def append_result(results: Path, rec: dict) -> None:
    with open(results, "a", encoding="utf-8") as f:
        fcntl.flock(f, fcntl.LOCK_EX)
        f.write(json.dumps(rec) + "\n")


def worker(args, wid: int) -> None:
    work = Path(args.work)
    plan = json.loads((work / "plan.json").read_text())
    order = plan["order"]
    hist = work / "history"
    wdir = work / f"w{wid}"
    tree = wdir / "src"
    out = wdir / "build.log"
    wdir.mkdir(parents=True, exist_ok=True)
    mbuild = REPO / "build" / f"occt-proof-m{wid}"
    sync_tree(tree, hist)
    log(f"w{wid}: OCCT at the tip")
    if not build_occt(tree, work, args.build_jobs, out):
        raise SystemExit(f"w{wid}: OCCT does not build at the tip ({out})")
    configure_mitcad(mbuild, args.preset, out)
    inc = installed_headers(mbuild)
    headers = port_headers(hist)
    changed = sync_headers(tree, inc, headers)
    if changed:
        log(f"w{wid}: installed headers differ from the port's tip: {', '.join(changed[:5])}")
    log(f"w{wid}: MitCad tests at the tip")
    if not build_mitcad(mbuild, args.targets, args.build_jobs, out):
        raise SystemExit(f"w{wid}: the MitCad tests do not build at the tip ({out})")
    tests = ctest_table(mbuild)
    libdir = occt_libs(tree)
    rev = Reverter(tree, order)
    logs = work / "logs"
    logs.mkdir(exist_ok=True)
    while True:
        item = locked_pop(work / "queue.json")
        if item is None:
            break
        group = item.split("+")
        names = plan["tests"][item]
        with_runs = plan["with"]
        # Each result keeps the with-runs of its tests: the table reads them
        # from there, not from the last run's plan, which names only its own.
        rec = {"item": item, "tests": names, "reverted": [], "state": "", "runs": {},
               "with": {n: with_runs[n] for n in names if n in with_runs}}
        t0 = time.time()
        seq = None
        try:
            seq = rev.revert(group)
            if seq is None:
                rec["state"] = "does not revert"
            elif len(seq) - len(group) > args.max_extra:
                rec["state"] = "does not revert alone"
                rec["needs"] = seq
            else:
                rec["reverted"] = seq
                built = build_occt(tree, work, args.build_jobs, out)
                # An #include a reverted patch adds and later patches rely on
                # is kept; a later patch using a function or variable a
                # reverted patch adds is reverted too.
                kept: list[list[str]] = []
                for _ in range(8):
                    if built:
                        break
                    restored = rev.keep_includes(seq, last_occt_errors[0], tree.parent / "build")
                    if restored:
                        kept += restored
                        built = build_occt(tree, work, args.build_jobs, out)
                        continue
                    users = rev.users(seq, last_occt_errors[0])
                    if not users or len(set(seq) | users) - len(group) > args.max_extra:
                        break
                    more = rev.revert(sorted(set(seq) | users))
                    if more is None:
                        break
                    seq = more
                    rec["reverted"] = seq
                    rev.apply_includes(kept)
                    built = build_occt(tree, work, args.build_jobs, out)
                if kept:
                    rec["kept_includes"] = kept
                if not built:
                    rec["state"] = "OCCT does not build"
                elif len(seq) - len(group) > args.max_extra:
                    rec["state"] = "does not revert alone"
                    rec["needs"] = seq
                else:
                    hdrs = sync_headers(tree, inc, headers)
                    rec["headers"] = hdrs
                    if not build_mitcad(mbuild, args.targets, args.build_jobs, out):
                        rec["state"] = "test does not build"
                        rec["failed"] = list(last_failed)
                    else:
                        rec["state"] = "ran"
                        for name in names:
                            if not Path(tests[name]["command"][0]).exists():
                                rec["runs"][name] = {"status": "not built", "why": ""}
                                continue
                            base = with_runs.get(name, {}).get("seconds", 60)
                            limit = min(args.max_timeout, max(args.min_timeout, args.timeout_factor * base))
                            lf = logs / f"without-{item}-{name}.log"
                            r = run_ctest(tests[name], libdir, lf, limit)
                            named = named_functions(item)
                            if r["status"] in ("failed", "timeout") and named:
                                found, unplaced = failing_tests(r["log"])
                                if unplaced and not found & named:
                                    # Each named test alone (the test runner's
                                    # MITCAD_TEST_ONLY), to see whether it is one that fails.
                                    r["only"] = {}
                                    for fn in sorted(named):
                                        lo = logs / f"without-{item}-{name}-{fn}.log"
                                        o = run_ctest(tests[name], libdir, lo, limit,
                                                      {"MITCAD_TEST_ONLY": only_selector(fn)})
                                        r["only"][fn] = {"status": o["status"], "log": o["log"]}
                            rec["runs"][name] = r
        except Exception as exc:  # one item's failure is recorded, the worker goes on
            rec["state"] = f"error: {str(exc).splitlines()[0][:200]}"
        rec["seconds"] = round(time.time() - t0)
        append_result(work / "results.jsonl", rec)
        summary = ", ".join(f"{n} {r['status']}" for n, r in rec["runs"].items()) or rec["state"]
        extra = f" (reverted {'+'.join(seq)})" if seq and len(seq) > 1 else ""
        log(f"w{wid}: {item}{extra}: {summary}")
    # Leave the trees at the tip.
    rev.reset()
    sync_headers(tree, inc, headers)
    build_occt(tree, work, args.build_jobs, out)
    build_mitcad(mbuild, args.targets, args.build_jobs, out)


# ---------------------------------------------------------------- verdicts and the table


_functions_cache: dict[str, set[str]] = {}


def named_functions(item: str) -> set[str]:
    """The test functions (test_...) the Test paragraphs of the item's patches name."""
    if item not in _functions_cache:
        files = {number(f): f for f in patch_order()}
        names: set[str] = set()
        for n in item.split("+"):
            if n in files:
                text = tests_of(n)
                for start, end in test_paragraphs(text):
                    # A file name (test_sweeps.cpp) is not a test function.
                    names |= set(re.findall(r"\b(test_\w+)\b(?!\.\w)", re.sub(r"\s+", " ", text[start:end])))
        _functions_cache[item] = names
    return _functions_cache[item]


_labels_cache: dict[str, set[str]] = {}


def guard_labels() -> dict[str, set[str]]:
    """The test functions each guarded("label", ...) call of geometry/tests runs.

    The test runners print a failure as "<label>: unexpected error", and
    MITCAD_TEST_ONLY selects the tests whose label contains it; a label is
    often words ("helix slivers within the bodies' faces") rather than the
    function's name."""
    if not _labels_cache:
        call = re.compile(r'\bguarded\(\s*"([^"]+)"\s*,')
        for src in sorted((REPO / "geometry" / "tests").rglob("*.cpp")):
            text = src.read_text(encoding="utf-8", errors="replace")
            for m in call.finditer(text):
                # The call's arguments up to its closing parenthesis.
                depth, i = 1, m.end()
                while i < len(text) and depth:
                    depth += {"(": 1, ")": -1}.get(text[i], 0)
                    i += 1
                names = set(re.findall(r"\b(test_\w+)\b", text[m.end():i]))
                _labels_cache.setdefault(m.group(1), set()).update(names)
    return _labels_cache


def only_selector(fn: str) -> str:
    """The MITCAD_TEST_ONLY value that runs the test function fn: the label
    of the guarded() call that runs it, or the name itself."""
    labels = [label for label, fns in guard_labels().items() if fn in fns]
    return labels[0] if len(labels) == 1 else fn


FUNCTION = re.compile(r"^(?:static |inline )?[\w:<>]+ (\w+)\(")


def enclosing_tests(path: str, line: int) -> set[str]:
    """The test functions a failing check at path:line lies in: its own
    function, or, for a helper, the test functions of that file calling it."""
    i = path.find("geometry/")
    src = REPO / path[i:] if i >= 0 else REPO / path
    if not src.exists():
        return set()
    lines = src.read_text(encoding="utf-8", errors="replace").split("\n")
    name = ""
    for k in range(min(line, len(lines)) - 1, -1, -1):
        m = FUNCTION.match(lines[k])
        if m:
            name = m.group(1)
            break
    if not name:
        return set()
    if name.startswith("test_"):
        return {name}
    callers = set()
    current = ""
    for ln in lines:
        m = FUNCTION.match(ln)
        if m:
            current = m.group(1)
        elif current.startswith("test_") and re.search(rf"\b{re.escape(name)}\(", ln):
            callers.add(current)
    return callers


def failing_tests(log: str) -> tuple[set[str], bool]:
    """The test functions whose checks fail in a run's log, and whether some
    failure could not be placed (a crash, a timeout, a check outside a test)."""
    try:
        text = Path(log).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return set(), True
    found: set[str] = set()
    unplaced = False
    for m in re.finditer(r"([\w./-]+\.cpp):(\d+): check failed", text):
        tests = enclosing_tests(m.group(1), int(m.group(2)))
        found |= tests
        unplaced |= not tests
    for m in re.finditer(r"^(.+?): unexpected error", text, re.M):
        # A guarded() label: the test functions its call runs. The runner
        # writes it to stderr, which can land in the middle of a stdout line
        # ("... off the path 3.07e-stirrup: unexpected error", mitcad#2121):
        # then the longest known label the text before it ends with.
        before = m.group(1)
        labels = guard_labels()
        if before not in labels:
            ends = [label for label in labels if before.endswith(label)]
            if ends:
                before = max(ends, key=len)
        found |= labels.get(before, {before})
    if not found:
        unplaced = True
    return found, unplaced


def verdict(rec: dict, with_runs: dict) -> tuple[str, dict]:
    """The patch's verdict and each test's without-status."""
    per = {}
    if rec["state"] == "does not revert":
        return "not shown: does not revert at the tip (entangled with over 40 later patches)", per
    if rec["state"] == "does not revert alone":
        extra = len(rec.get("needs", [])) - len(rec["item"].split("+"))
        return f"not shown: reverting it at the tip needs {extra} later patches", per
    if rec["state"] == "OCCT does not build":
        return "not shown: OCCT does not build with it reverted", per
    if rec["state"].startswith("error"):
        return f"not shown: {rec['state']}", per
    if rec["state"] == "test does not build":
        what = ", ".join(rec.get("failed", [])[:3])
        for n in rec["tests"]:
            per[n] = f"does not build ({what})" if what else "does not build"
        if len([n for n in rec.get("reverted", []) if n not in rec["item"].split("+")]) > SINGLE_OUT:
            return "not shown: the test does not build with the later patches reverted", per
        return "fails without the patch (the test does not build)", per
    fails = passes = bad = unrun = 0
    elsewhere: set[str] = set()
    named = named_functions(rec["item"])
    with_runs = {**with_runs, **rec.get("with", {})}
    for n in rec["tests"]:
        w = with_runs.get(n, {}).get("status")
        wo = rec["runs"].get(n, {}).get("status")
        per[n] = wo or "-"
        if w is None:
            unrun += 1
        elif w != "passed":
            bad += 1
        elif wo in ("failed", "timeout") and rec["runs"][n].get("only"):
            alone = {}
            for fn, o in rec["runs"][n]["only"].items():
                if o["status"] in ("failed", "timeout"):
                    f2, u2 = failing_tests(o["log"])
                    alone[fn] = u2 or fn in f2
                else:
                    alone[fn] = False
            hit = sorted(fn for fn, bad_fn in alone.items() if bad_fn)
            if hit:
                fails += 1
                per[n] += " (alone: " + ", ".join(hit) + " fails)"
            else:
                elsewhere.add("-")
                per[n] += " (alone, each named test passes)"
        elif wo in ("failed", "timeout"):
            found, unplaced = failing_tests(rec["runs"][n].get("log", ""))
            if not named or unplaced or found & named:
                fails += 1
                if found & named:
                    per[n] += " in " + ", ".join(sorted(found & named))
                elif named:
                    per[n] += " (not placed in a test)"
            else:
                elsewhere |= found
                per[n] += " in other tests only: " + ", ".join(sorted(found))
        elif wo == "passed":
            passes += 1
    others = [n for n in rec.get("reverted", []) if n not in rec["item"].split("+")]
    if fails and len(others) > SINGLE_OUT:
        return f"fails with {len(others)} later patches reverted too: not singled out", per
    if fails:
        return "fails without the patch", per
    if elsewhere:
        return "passes without the patch: the named test does not prove it (its ctest fails in other tests)", per
    if bad and not passes:
        return "not shown: the test does not pass with the patch", per
    if unrun and not passes:
        return "not shown: the test was not run with the patch", per
    if passes:
        return "passes without the patch: the test does not prove it", per
    return "not shown: no test ran", per


def load_results(work: Path) -> dict:
    res = {}
    p = work / "results.jsonl"
    if p.exists():
        for line in p.read_text().splitlines():
            if line.strip():
                rec = json.loads(line)
                res[rec["item"]] = rec
    return res


def parse_table(path: Path) -> dict:
    """The entries of an earlier table by item (table_entry's shape), so that
    a table written without some results keeps their rows."""
    out: dict = {}
    if not path.exists():
        return out
    cols = None
    cur = None
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if cells and cells[0] == "Patch":
            cols = cells
            continue
        if cols is None or set(line) <= set("|-: "):
            continue
        row = dict(zip(cols, cells))
        if row.get("Patch"):
            cur = out.setdefault(row["Patch"], {"revd": row.get("Reverted", "-"), "verdict": row.get("Verdict", "-"),
                                                "place": row.get("In chain", "-") or "-",
                                                "cverdict": row.get("In-chain verdict", "-") or "-",
                                                "tests": [], "tip": {}, "chain": {}})
        if cur is None:
            continue
        name = row.get("Test", "-").strip("`")
        cur["tests"].append(name)
        cur["tip"][name] = (row.get("With", "-"), row.get("Without", "-"))
        cur["chain"][name] = row.get("Before / with it", "-") or "-"
    return out


def table_entry(item: str, rec: dict, with_runs: dict) -> dict:
    """A tip result as the table shows it."""
    v, per = verdict(rec, with_runs)
    others = [n for n in rec.get("reverted", []) if n not in item.split("+")]
    revd = ("with " + ", ".join(others)) if others else "alone"
    if rec["state"] == "does not revert":
        revd = "-"
    elif rec["state"] == "does not revert alone":
        revd = f"needs {len(rec.get('needs', [])) - len(item.split('+'))} later"
    if rec.get("kept_includes"):
        revd += "; kept " + ", ".join(sorted({ln.split("<")[1].rstrip(">") for _, ln in rec["kept_includes"]}))
    tip = {}
    tests = rec["tests"] or ["-"]
    with_runs = {**with_runs, **rec.get("with", {})}
    for n in tests:
        wo = rec["runs"].get(n, {})
        wotxt = per.get(n, "-")
        if wo.get("why"):
            wotxt += f" ({re.sub(r'^(\.\./)+', '', wo['why']).replace('|', '/')})"
        tip[n] = (with_runs.get(n, {}).get("status", "-"), wotxt)
    return {"revd": revd, "verdict": v, "tests": list(tests), "tip": tip}


def chain_entry(rec: dict) -> dict:
    """A chain result as the table shows it."""
    cv, per = chain_verdict(rec)
    place = f"after {rec['base']}"
    if rec.get("carried"):
        place += "; carried " + carried_text(rec["carried"])
    if rec.get("needs"):
        place += f"; needs {len(rec['needs'])} later"
    return {"place": place, "cverdict": cv, "tests": list(rec["tests"] or ["-"]),
            "chain": {n: per.get(n, "-").replace("|", "/") for n in rec["tests"] or ["-"]}}


def table_entries(args, plan: dict, results: dict) -> dict:
    """The earlier table's entries, replaced by the tip and chain results there are."""
    entries = parse_table(Path(args.table))
    blank = {"revd": "-", "verdict": "-", "place": "-", "cverdict": "-", "tests": [], "tip": {}, "chain": {}}
    for item, rec in results.items():
        e = entries.setdefault(item, json.loads(json.dumps(blank)))
        new = table_entry(item, rec, plan.get("with", {}))
        e.update({k: new[k] for k in ("revd", "verdict", "tip")})
        # An earlier row stays only for the chain's result on it.
        e["tests"] = new["tests"] + [t for t in e["tests"]
                                     if t not in new["tests"] and e["chain"].get(t, "-") not in ("", "-")]
    for item, rec in load_chain_results(Path(args.work)).items():
        e = entries.setdefault(item, json.loads(json.dumps(blank)))
        new = chain_entry(rec)
        e.update({k: new[k] for k in ("place", "cverdict", "chain")})
        # An earlier row stays only for the tip's result on it.
        kept = [t for t in e["tests"] if t in new["tests"] or e["tip"].get(t, ("-", "-")) != ("-", "-")]
        e["tests"] = kept + [t for t in new["tests"] if t not in kept]
    # A patch whose section in the test document says "Test: none,
    # because ..." has no test to run: an earlier row of its test is no
    # longer its proof (mitcad#2121, the section since mitcad#2133).
    for f in patch_order():
        if re.search(r"^Tests?:\s*none\b", tests_of(number(f)), re.M):
            entries[number(f)] = {**json.loads(json.dumps(blank)), "verdict": "Test: none (its section says why)",
                                  "tests": ["-"]}
    return entries


def write_table(args, plan: dict, results: dict) -> Path:
    table = Path(args.table)
    old_date = re.search(r"^Made (\d{4}-\d\d-\d\d)", table.read_text(encoding="utf-8"), re.M) if table.exists() else None
    entries = table_entries(args, plan, results)
    rows = []
    counts: dict[str, int] = {}
    ccounts: dict[str, int] = {}
    for item in sorted(entries, key=lambda s: s.split("+")[0]):
        e = entries[item]
        v, cv = e["verdict"], e["cverdict"]
        if v not in ("", "-"):
            key = re.sub(r"\d+ later", "several later", v.split(":")[0])
            if v.startswith("Test: none"):
                key = "Test: none"
            counts[key] = counts.get(key, 0) + 1
        if cv not in ("", "-"):
            key = cv.split(":")[0]
            ccounts[key] = ccounts.get(key, 0) + 1
        for i, n in enumerate(e["tests"] or ["-"]):
            wtxt, wotxt = e["tip"].get(n, ("-", "-"))
            ctxt = e["chain"].get(n, "-")
            first = i == 0
            rows.append(f"| {item if first else ''} | {e['revd'] if first else ''} | `{n}` | {wtxt} | {wotxt} | "
                        f"{v if first else ''} | {e['place'] if first else ''} | {ctxt} | {cv if first else ''} |")
    tested_items = [i for i, e in entries.items() if not e["verdict"].startswith("Test: none")]
    proven_items = [i for i in tested_items
                    if entries[i]["verdict"].startswith("fails without") or entries[i]["cverdict"].startswith("fails")]
    tested, proven = len(tested_items), len(proven_items)
    unproven_items = sorted(set(tested_items) - set(proven_items), key=lambda s: s.split("+")[0])
    date = plan.get("date") or (old_date.group(1) if old_date else datetime.date.today().isoformat())
    lines = [
        "<!-- SPDX-License-Identifier: MIT -->",
        "# The port patches' tests with and without each patch",
        "",
        f"Made {date} by `{TOOL}` (mitcad#1986): OCCT 8.0.1 with every",
        "port patch (*with*), then with one patch reverted at the port's tip",
        "(*without*). A patch that does not revert alone is reverted together",
        "with the later patches that change the same lines, named in the second",
        "column; an `#include` it added that later patches rely on is kept",
        "(*kept*). Only the changed OCCT libraries, and the MitCad tests that",
        "include a changed OCCT header, are rebuilt; the ctests the patch's",
        "`Test:` paragraph in [opencascade-patch-tests.md](opencascade-patch-tests.md)",
        "names run against them. A failure counts for the",
        "patch when it lies in a test function the paragraph names (run alone",
        "where the output does not place it); with more than "
        f"{SINGLE_OUT} later patches",
        "reverted too, the test does not single the patch out.",
        "",
        "The last three columns show a patch at its own place in the chain",
        "(`--in-chain`, mitcad#2121): OCCT with the patches before it and with",
        "the patches up to it, no later patch reverted, the MitCad tests built",
        "against each state's headers (*before / with it*). Where the MitCad",
        "code the tests need uses an API a later patch adds, that patch's",
        "declarations (its changes to the headers) are applied to both states",
        "(*carried … (its declarations)*), a function only declared there bound",
        "to a stub that ends the run if it is called; where they do not apply or",
        "OCCT does not build with them, the patch is cherry-picked whole",
        "(*carried*). Where the patch itself adds the API, the test does not",
        "build without the patch. A suite part is linked without the parts of",
        "its executable that do not build at that state.",
        "",
        "Verdicts: " + "; ".join(f"{k}: {v}" for k, v in sorted(counts.items())) + ".",
        "",
        "In-chain verdicts: " + ("; ".join(f"{k}: {v}" for k, v in sorted(ccounts.items())) or "none yet") + ".",
        "",
        f"Shown to fail without the patch at the tip or at its place in the chain: {proven} of the "
        f"{tested} patches with a test (the others: {', '.join(unproven_items) or 'none'}).",
        "",
        "| Patch | Reverted | Test | With | Without | Verdict | In chain | Before / with it | In-chain verdict |",
        "|---|---|---|---|---|---|---|---|---|",
        *rows,
        "",
    ]
    table.write_text("\n".join(lines))
    return table


# ---------------------------------------------------------------- test document updates


# Up to this many later patches reverted with one, a failure still counts as
# the patch's own; with more, the sentence says the test does not single it out.
SINGLE_OUT = 5

# The Test paragraph's sentence on the before/after proof: as the port's
# headers wrote it before mitcad#2133 moved the paragraphs into the test
# document, or as this tool wrote it (so that a later run rewrites it).
SENTENCE = re.compile(
    r"(?:A new test: )?[Ii]ts\s+failure\s+without\s+the\s+patch(?P<mid>.*?)\s+(?:is|was)\s+still\s+to\s+be\s+shown\s+in"
    r"\s+the\s+port's\s+before/after\s+run(?:\s+\(mitcad#1986\))?\."
    r"|(?:It\s+fails|It\s+passes|The\s+named\s+test\s+passes|Its\s+failure\s+without\s+the\s+patch\s+is\s+still"
    r"\s+to\s+be\s+shown:)[^.]*?(?:\.[^.\s][^.]*?)*?tools/occt-port-proof\.py[^)]*\)\.", re.S)


def update_tests(args, plan: dict, results: dict) -> list[str]:
    """Rewrites the proof sentences of the results' patches (of those given,
    where patches are given) in the test document; no .patch file changes."""
    date = plan.get("date", datetime.date.today().isoformat())
    chain = load_chain_results(Path(args.work))
    entries = table_entries(args, plan, results)
    wanted = {n for p in args.patches for n in p.split("+")}
    doc = tests_doc()
    text = doc.read_text(encoding="utf-8")
    changed = []
    for item in sorted(set(results) | set(chain)):
        if wanted and not wanted & set(item.split("+")):
            continue
        new = tip_sentence(item, results[item], plan, date) if item in results else None
        # The chain's result where the tip did not prove the patch.
        tip_v = entries.get(item, {}).get("verdict", "-")
        if item in chain and not tip_v.startswith("fails without the patch"):
            new = chain_sentence(chain[item]) or new
        if new is None:
            continue
        for n in item.split("+"):
            sec_m = next((m for m in SECTION.finditer(text) if m.group(1) == n), None)
            if sec_m is None:
                log(f"{n}: no section in {doc}: its proof sentence is not written")
                continue
            sec = sec_m.group(2)
            spans = test_paragraphs(sec)
            done = False
            for start, end in reversed(spans):
                para = sec[start:end]
                m = SENTENCE.search(para)
                if not m:
                    continue
                # The lines before the sentence's line stay; from there to the
                # paragraph's end the text is wrapped again.
                line = para.rfind("\n", 0, m.start()) + 1
                body = para.rstrip()
                tail = para[len(body):]
                rest = body[line:m.start()] + new + body[m.end():]
                flat = re.sub(r"\s+", " ", rest).strip()
                wrapped = "\n".join(textwrap.wrap(flat, width=74, break_long_words=False, break_on_hyphens=False))
                sec = sec[:start] + para[:line] + wrapped + tail + sec[end:]
                done = True
                break
            if done:
                text = text[:sec_m.start(2)] + sec + text[sec_m.end(2):]
                changed.append(n)
    if changed:
        doc.write_text(text, encoding="utf-8")
        set_tests_doc(str(doc))
    return changed


def chain_sentence(rec: dict) -> str | None:
    """The Test paragraph's sentence for a decisive result in the chain, else None."""
    v, _ = chain_verdict(rec)
    date = rec.get("date", datetime.date.today().isoformat())
    how = f"shown {date} by {TOOL} --in-chain"
    if rec.get("carried"):
        # No parentheses inside: the sentence ends at its first ")." (SENTENCE).
        whole = [p for p in rec["carried"] if not p.startswith(API_ONLY)]
        decl = [pick_number(p) for p in rec["carried"] if p.startswith(API_ONLY)]
        parts = ([", ".join(whole)] if whole else []) + ([f"the declarations of {', '.join(decl)}"] if decl else [])
        verb = "adds" if len(rec["carried"]) == 1 else "add"
        how += f", with {' and '.join(parts)} carried, which {verb} API the MitCad code uses"
    if v.startswith("fails without the patch at its place (the test does not build"):
        return (f"It fails without the patch at its place in the chain: with the patches before it, the "
                f"test does not build without the API the patch adds ({how}).")
    if v.startswith("fails without the patch at its place"):
        return (f"It fails without the patch at its place in the chain: OCCT with the patches before it, "
                f"without and with this one ({how}).")
    if v.startswith("passes without the patch at its place") and "other tests" in v:
        return (f"The named test passes without the patch at its place in the chain too, so it does not "
                f"prove it; its ctest fails there in other tests only ({how}).")
    if v.startswith("passes without the patch at its place"):
        return f"It passes without the patch at its place in the chain too: the test does not prove it ({how})."
    return None


def tip_sentence(item: str, rec: dict, plan: dict, date: str) -> str | None:
    """The Test paragraph's sentence for a result at the tip, or None to leave it."""
    v, _ = verdict(rec, plan["with"])
    others = [n for n in rec.get("reverted", []) if n not in item.split("+")]
    how = f"shown {date} by {TOOL}"
    many = len(others) > SINGLE_OUT
    if many:
        how += f", reverted together with {len(others)} later patches that change the same lines"
    elif others:
        verb = "changes" if len(others) == 1 else "change"
        how += f", reverted together with {', '.join(sorted(others))}, which {verb} the same lines"
    if v.startswith("fails with ") and many:
        new = (f"It fails with this patch and {len(others)} later patches that change the same lines "
               f"reverted together, so the test does not single this patch out (shown {date} by "
               f"{TOOL}; the patches are listed in third_party/vcpkg-ports/docs/opencascade-before-after.md).")
    elif v.startswith("fails without the patch (the test does not build)"):
        new = f"It fails without the patch: the test does not build without it ({how})."
    elif v.startswith("fails without"):
        new = f"It fails without the patch ({how})."
    elif v.startswith("passes without") and "other tests" in v:
        new = (f"The named test passes without the patch, so it does not prove it; its ctest fails "
               f"without the patch in other tests only ({how}).")
    elif v.startswith("passes without"):
        new = f"It passes without the patch: the test does not prove it ({how})."
    elif rec["state"] in ("does not revert", "does not revert alone"):
        k = len(rec.get("needs", [])) - len(item.split("+"))
        many = f"{k}" if rec["state"] == "does not revert alone" else "more than 40"
        new = (f"Its failure without the patch is still to be shown: the patch does not revert at the "
               f"port's tip without the {many} later patches that change the same lines (tried {date} "
               f"by {TOOL}).")
    elif rec["state"] in ("OCCT does not build", "test does not build"):
        what = "OCCT" if rec["state"] == "OCCT does not build" else "the test"
        new = (f"Its failure without the patch is still to be shown: with it and the {len(others)} later "
               f"patches that change the same lines reverted at the port's tip, {what} does not build "
               f"(tried {date} by {TOOL}).")
    else:
        return None
    return new


# ---------------------------------------------------------------- in the chain (mitcad#2121)


# Raised when the way a state is built or judged changes, so that cached
# states and results of an older tool are not reused.
CHAIN_VERSION = 16
HEADER_GLOBS = ["*.hxx", "*.lxx", "*.gxx", "*.h"]
# Words an error message quotes that are no API a patch adds.
CPP_WORDS = {"const", "class", "struct", "int", "double", "bool", "void", "auto", "char", "unsigned", "long",
             "float", "enum", "typename", "operator", "std", "this", "nullptr", "return", "static", "virtual"}
# The objects of a suite executable that only dispatch to its parts.
DISPATCHERS = re.compile(r"(?:tests/test_main|tests/port/main)\.cpp")
# What the stub bound to a missing function writes when it is called.
STUB_CALLED = "occt-port-proof: a function no patch at this state defines was called"


def chain_stamp(order: list[str]) -> str:
    """The port's code and the MitCad sources the chain's cached builds and
    runs depend on: by content, so that edited patch headers or other
    commits keep them."""
    h = hashlib.sha256(f"chain {CHAIN_VERSION}".encode())
    for f in order:
        h.update(f.encode())
        h.update((PORT / f).read_text(encoding="utf-8", errors="replace")[len(header(f)):].encode())
    paths = ["geometry", "core/cpp", "cmake", "CMakeLists.txt", "CMakePresets.json", "vcpkg.json"]
    h.update(run(["git", "-C", REPO, "ls-files", "-s", "--", *paths]).stdout.encode())
    h.update(run(["git", "-C", REPO, "diff", "HEAD", "--", *paths]).stdout.encode())
    return h.hexdigest()[:16]


# A carried patch of which only the changes to the headers are applied (its
# declarations: an enumerator, a function, a member), "h:<number>"; the rest
# of it stays out of the state (mitcad#2121).
API_ONLY = "h:"


def pick_number(pick: str) -> str:
    return pick.removeprefix(API_ONLY)


def pick_order(nums: list[str]):
    return lambda pick: (nums.index(pick_number(pick)), pick)


def carried_text(picks: list[str]) -> str:
    """The carried patches as the table and the test document name them."""
    return ", ".join(pick_number(p) + (" (its declarations)" if p.startswith(API_ONLY) else "") for p in picks)


def chain_state(nums: list[str], base_idx: int, picks) -> tuple[str, list[str]]:
    """A state of the chain: the history's commit after nums[base_idx] and the
    later patches cherry-picked onto it, in order; picks that directly follow
    the base are folded into it, so that each state has one name. A patch
    carried whole needs no carry of its declarations."""
    full = {p for p in picks if not p.startswith(API_ONLY)}
    picks = sorted({p for p in picks if pick_number(p) not in full or p in full}, key=pick_order(nums))
    while picks and not picks[0].startswith(API_ONLY) and nums.index(picks[0]) == base_idx + 1:
        base_idx += 1
        picks.pop(0)
    return (f"p-{nums[base_idx]}" if base_idx >= 0 else "base"), picks


def state_key(tag: str, picks: list[str]) -> str:
    return tag + "".join("+" + p for p in picks)


def tag_index(nums: list[str], tag: str) -> int:
    return nums.index(tag[2:]) if tag.startswith("p-") else -1


def chain_tree(tree: Path, hist: Path) -> None:
    """A clone of the history, left at whatever state it was in."""
    if not (tree / ".git").exists():
        run(["git", "clone", "-q", "--no-checkout", hist, tree])
    git(tree, "config", "gc.auto", "0")
    git(tree, "fetch", "-q", "--force", "--tags", hist)


def chain_ctest_table(build: Path) -> dict:
    """ctest's table, with the command of each test whose executable is not
    built yet (ctest leaves it out) taken from the CTestTestfile.cmake files."""
    tests = ctest_table(build)
    if all("command" in t for t in tests.values()):
        return tests
    quoted = re.compile(r'"((?:[^"\\]|\\.)*)"')
    for f in build.rglob("CTestTestfile.cmake"):
        for m in re.finditer(r"^add_test\(\[=\[(.*?)\]=\]\s+(.*)\)\s*$", f.read_text(errors="replace"), re.M):
            t = tests.get(m.group(1))
            if t is not None and "command" not in t:
                t["command"] = [re.sub(r"\\(.)", r"\1", a) for a in quoted.findall(m.group(2))]
    return tests


def test_target(test: dict, build: Path) -> str | None:
    """The ninja target of a ctest's executable, None when it runs no
    executable of the build."""
    try:
        return str(Path(test["command"][0]).resolve().relative_to(build.resolve()))
    except ValueError:
        return None


def exe_dir(target: str) -> str:
    """The object folder CMake gives an executable target."""
    parent = str(Path(target).parent)
    return ("" if parent == "." else parent + "/") + f"CMakeFiles/{Path(target).name}.dir/"


def own_sources(test: dict) -> set[str] | None:
    """The sources (relative to the target's folder) a suite part's run needs
    of its executable; None where the executable has no parts."""
    cmd = test["command"]
    if "--part" not in cmd or cmd.index("--part") + 1 >= len(cmd):
        return None
    part = cmd[cmd.index("--part") + 1]
    exe = Path(cmd[0]).name
    if exe == "test_geometry":
        table = (REPO / "geometry" / "tests" / "test_main.cpp").read_text(encoding="utf-8", errors="replace")
        m = re.search(rf'\{{"{re.escape(part)}",\s*test::(\w+)\}}', table)
        if not m:
            return None
        for src in sorted((REPO / "geometry" / "tests").glob("*.cpp")):
            if re.search(rf"\bvoid\s+{m.group(1)}\s*\(\s*\)\s*\{{", src.read_text(encoding="utf-8", errors="replace")):
                return {"tests/test_main.cpp", f"tests/{src.name}"}
        return None
    if exe == "test_port_patches":
        files = sorted((REPO / "geometry" / "tests" / "port").glob(f"p{part}-*.cpp"))
        return {"tests/port/main.cpp", *(f"tests/port/{f.name}" for f in files)} if files else None
    return None


def failed_blocks(text: str) -> dict[str, str]:
    """Each failed ninja edge's output (the compiler's or linker's), by output."""
    blocks: dict[str, list[str]] = {}
    cur = None
    for ln in text.splitlines():
        m = re.match(r"^FAILED: (?:\[code=\d+\] )?(\S+)", ln)
        if m:
            cur = m.group(1)
            blocks[cur] = []
        elif re.match(r"^(\[\d+/\d+\] |ninja: |\$ )", ln):
            cur = None
        elif cur is not None:
            blocks[cur].append(ln)
    return {k: "\n".join(v) for k, v in blocks.items()}


def target_inputs(build: Path, target: str) -> set[str]:
    """Every input of a ninja target, transitively."""
    return set(run(["ninja", "-C", build, "-t", "inputs", target], check=False).stdout.split())


def built(build: Path, target: str, failed: dict) -> bool:
    """Whether a target is built after a `ninja -k 0` whose failed edges are
    `failed`: it exists and none of its inputs failed. (A dry run cannot tell:
    with CONFIGURE_DEPENDS globs it always plans to re-run CMake.)"""
    if target in failed:
        return False
    if not (build / target).exists() and not target.endswith("all"):
        return False
    return not target_inputs(build, target) & set(failed)


def link_command(build: Path, target: str) -> list[str] | None:
    """The command that links an executable target (as ninja would run it)."""
    lines = run(["ninja", "-C", build, "-t", "commands", target], check=False).stdout.strip().splitlines()
    if not lines:
        return None
    for seg in lines[-1].split(" && "):
        words = shlex.split(seg)
        if "-o" in words and words.index("-o") + 1 < len(words) and words[words.index("-o") + 1] == target:
            return words
    return None


def occt_undefined(libdir: Path, binaries: list[str] = ()) -> list[str]:
    """The symbols OCCT's libraries in libdir (and the given binaries) need
    and neither they nor the system libraries they load define."""
    libs = sorted(str(f) for f in libdir.glob("libTK*.so"))
    if not libs:
        return []
    need: set[str] = set()
    have: set[str] = set()
    for lib in [*libs, *binaries]:
        need |= set(run(["nm", "-D", "--undefined-only", "--format=just-symbols", lib]).stdout.split())
        have |= set(run(["nm", "-D", "--defined-only", "--format=just-symbols", lib]).stdout.split())
    # The other libraries they load: the system's, and the installed
    # OCCT libraries this tree does not build (Visualization, DataExchange);
    # the ones it builds are taken from libdir only.
    ours = {Path(f).name.split(".so")[0] for f in libs}
    system = set()
    for ln in run(["ldd", *libs, *binaries], check=False).stdout.splitlines():
        m = re.search(r"(\S+) => (/\S+)", ln)
        if m and m.group(1).split(".so")[0] not in ours:
            system.add(m.group(2))
    for lib in sorted(system):
        have |= set(run(["nm", "-D", "--defined-only", "--format=just-symbols", lib], check=False).stdout.split())
    # Weak symbols and versioned names (name@VERSION) are the system's.
    return sorted(n.split("@")[0] for n in need - have if n.startswith("_Z") and "@" not in n)


def undefined_only_in_dispatchers(text: str) -> bool:
    """Whether a link's undefined references all come from the objects that
    only dispatch to a suite's parts."""
    ctx = ""
    found = False
    for ln in text.splitlines():
        if "in function" in ln:
            ctx = ln
        if "undefined reference" in ln:
            found = True
            where = ln if re.search(r"\.(?:o|cpp):", ln) else ctx
            if not DISPATCHERS.search(where):
                return False
    return found


def failure_counts(run_: dict, named: set[str]) -> tuple[bool, str]:
    """Whether a failing run counts for the patch (as verdict() decides at
    the tip), and a note on where it failed."""
    only = {fn: o for fn, o in run_.get("only", {}).items() if fn in named}
    if only:
        hit = []
        for fn, o in sorted(only.items()):
            if o["status"] in ("failed", "timeout"):
                f2, u2 = failing_tests(o["log"])
                if u2 or fn in f2:
                    hit.append(fn)
        if hit:
            return True, " (alone: " + ", ".join(hit) + " fails)"
        return False, " (alone, each named test passes)"
    found, unplaced = failing_tests(run_.get("log", ""))
    if not named or unplaced or found & named:
        if found & named:
            return True, " in " + ", ".join(sorted(found & named))
        return True, " (not placed in a test)" if named else ""
    return False, " in other tests only: " + ", ".join(sorted(found))


def chain_verdict(rec: dict) -> tuple[str, dict]:
    """A patch's verdict at its place in the chain, and each test's
    'before / with it' text."""
    per: dict[str, str] = {}
    st = rec["state"]
    if st != "ran":
        return f"not shown: {st}", per
    named = named_functions(rec["item"])
    fails = api = passes = elsewhere = no_pass = no_build = stub = 0
    for n in rec["tests"]:
        w = rec["runs"]["with"].get(n, {})
        wo = rec["runs"]["without"].get(n, {})
        ws, wos = w.get("status", "-"), wo.get("status", "-")
        wtxt = ws + (f" ({w['why']})" if w.get("why") else "")
        alone = {fn: o for fn, o in w.get("only", {}).items() if fn in named}
        if ws in ("failed", "timeout", "stub called") and named and named <= set(alone):
            # The ctest fails with the patch in tests other than the named ones.
            if all(o["status"] == "passed" for o in alone.values()):
                ws = "passed"
                wtxt = "passed alone (" + ", ".join(sorted(alone)) + ")"
        alone_wo = {fn: o for fn, o in wo.get("only", {}).items() if fn in named}
        if wos == "stub called" and named and named <= set(alone_wo):
            # Another test of the ctest called a stub: the named ones alone decide.
            if any(o["status"] in ("failed", "timeout") for o in alone_wo.values()):
                wos = "failed"
            elif all(o["status"] == "passed" for o in alone_wo.values()):
                wos = "passed"
        wotxt = wos + (f" ({wo['why']})" if wo.get("why") and wos != "failed" else "")
        if "stub called" in (ws, wos):
            stub += 1
        elif ws == "not built":
            no_build += 1
        elif ws != "passed":
            no_pass += 1
        elif wos == "not built":
            api += 1
        elif wos in ("failed", "timeout"):
            hit, note = failure_counts(wo, named)
            wotxt = wos + note + (f" ({re.sub(r'^(\.\./)+', '', wo['why'])})" if wo.get("why") else "")
            if hit:
                fails += 1
            else:
                elsewhere += 1
        elif wos == "passed":
            passes += 1
        per[n] = f"{wotxt} / {wtxt}"
    if fails:
        return "fails without the patch at its place", per
    if api:
        return "fails without the patch at its place (the test does not build without it)", per
    if elsewhere:
        return ("passes without the patch at its place: the named test does not prove it "
                "(its ctest fails in other tests)"), per
    if passes:
        return "passes without the patch at its place: the test does not prove it", per
    if stub:
        return ("not shown: the test calls a function that a later patch, carried as its declarations "
                "only, defines"), per
    if no_pass:
        return "not shown: the test does not pass at its place", per
    if no_build:
        more = f" (needs {len(rec['needs'])} later patches' API)" if rec.get("needs") else ""
        return f"not shown: the test does not build at its place{more}", per
    return "not shown: no test ran", per


def load_chain_results(work: Path) -> dict:
    res = {}
    p = work / "chain-results.jsonl"
    if p.exists():
        for line in p.read_text().splitlines():
            if line.strip():
                rec = json.loads(line)
                res[rec["item"]] = rec
    return res


class ChainCache:
    """Every state's build and test runs, shared by the workers and kept
    for the next run while the port and the MitCad sources are the same."""

    def __init__(self, work: Path, stamp: str):
        self.states_file = work / "chain-states.jsonl"
        self.runs_file = work / "chain-runs.jsonl"
        self.stamp = stamp

    def _load(self, f: Path) -> list[dict]:
        if not f.exists():
            return []
        out = []
        for line in f.read_text().splitlines():
            if line.strip():
                rec = json.loads(line)
                if rec.get("stamp") == self.stamp:
                    out.append(rec)
        return out

    def state(self, key: str) -> dict | None:
        info = None
        for r in self._load(self.states_file):
            if r["key"] == key:
                info = r if info is None else {**info, **r, "tests": {**info["tests"], **r["tests"]}}
        return info

    def run(self, key: str, name: str) -> dict | None:
        res = None
        for r in self._load(self.runs_file):
            if r["key"] == key and r["test"] == name:
                res = r["run"]
        return res

    def put_state(self, info: dict) -> None:
        append_result(self.states_file, {**info, "stamp": self.stamp})

    def put_run(self, key: str, name: str, run_: dict) -> None:
        append_result(self.runs_file, {"key": key, "test": name, "run": run_, "stamp": self.stamp})


class ChainWorker:
    """One worker of the chain: its own OCCT tree and MitCad build, moved
    forwards from state to state."""

    def __init__(self, args, wid: int):
        self.args = args
        self.work = Path(args.work)
        self.plan = json.loads((self.work / "chain-plan.json").read_text())
        self.nums = [number(f) for f in self.plan["order"]]
        self.hist = self.work / "history"
        self.wdir = self.work / f"c{wid}"
        self.wdir.mkdir(parents=True, exist_ok=True)
        self.tree = self.wdir / "src"
        self.out = self.wdir / "build.log"
        self.mbuild = REPO / "build" / f"occt-chain-m{wid}"
        self.wid = wid
        self.cache = ChainCache(self.work, self.plan["stamp"])
        self.logs = self.work / "chain-logs"
        self.logs.mkdir(exist_ok=True)
        chain_tree(self.tree, self.hist)
        configure_mitcad(self.mbuild, args.preset, self.out)
        self.inc = installed_headers(self.mbuild)
        self.headers = port_headers(self.hist)
        self.tests = chain_ctest_table(self.mbuild)
        self.current: str | None = None  # the state the tree and both builds are at
        self.info: dict = {}
        self.added: dict[str, str] = {}
        self.conflict_files: list[str] = []
        self.api_picks: list[str] = []
        self.state_numbers: set[str] = set()

    # -- states

    def prepare(self, tag: str, picks: list[str]) -> str | None:
        """Puts the tree at a state; the patch that does not cherry-pick, if one."""
        git(self.tree, "cherry-pick", "--abort", check=False)
        git(self.tree, "checkout", "-q", "-f", "--detach", tag)
        git(self.tree, "reset", "-q", "--hard", tag)
        for n in picks:
            if n.startswith(API_ONLY):
                if not self.apply_declarations(pick_number(n)):
                    git(self.tree, "reset", "-q", "--hard", tag)
                    self.conflict_files = []
                    return n
                continue
            p = git(self.tree, "-c", "user.name=port", "-c", "user.email=port@localhost", "cherry-pick",
                    "--allow-empty", "--keep-redundant-commits", f"p-{n}", check=False)
            if p.returncode != 0:
                self.conflict_files = git(self.tree, "diff", "--name-only", "--diff-filter=U",
                                          check=False).stdout.split()
                git(self.tree, "cherry-pick", "--abort", check=False)
                git(self.tree, "reset", "-q", "--hard", tag)
                return n
        return None

    def apply_declarations(self, n: str) -> bool:
        """Applies a later patch's changes to the headers alone (three-way) and
        commits them; False where they do not apply at this state."""
        diff = git(self.tree, "diff", f"p-{n}~1", f"p-{n}", "--", *HEADER_GLOBS).stdout
        if diff.strip():
            f = self.wdir / "declarations.diff"
            f.write_text(diff)
            if git(self.tree, "apply", "-3", "--whitespace=nowarn", f, check=False).returncode != 0:
                # Declarations conflict where the patches between wrote the
                # lines beside them: each conflict keeps this state's lines and
                # adds the patch's added lines it lacks (OCCT's build then
                # shows whether that holds).
                added = {ln[1:] for ln in diff.splitlines() if ln.startswith("+") and not ln.startswith("+++")}
                conflicted = git(self.tree, "diff", "--name-only", "--diff-filter=U", check=False).stdout.split()
                if not conflicted:
                    return False
                for path in conflicted:
                    if not self.union(self.tree / path, added):
                        return False
                    git(self.tree, "add", path)
        git(self.tree, "add", "-A")
        git(self.tree, "-c", "user.name=port", "-c", "user.email=port@localhost", "commit", "-q", "--allow-empty",
            "-m", f"declarations of {n}")
        return True

    @staticmethod
    def union(path: Path, added: set[str]) -> bool:
        """Resolves a file's conflicts (merge style) as this state's side plus
        the other side's lines that the patch adds and this side lacks."""
        out: list[str] = []
        ours: list[str] = []
        theirs: list[str] = []
        where = ""
        for ln in path.read_text(encoding="utf-8", errors="replace").split("\n"):
            if ln.startswith("<<<<<<< "):
                where, ours, theirs = "ours", [], []
            elif ln == "=======" and where == "ours":
                where = "theirs"
            elif ln.startswith(">>>>>>> ") and where == "theirs":
                out += ours + [t for t in theirs if t in added and t not in ours]
                where = ""
            elif where == "ours":
                ours.append(ln)
            elif where == "theirs":
                theirs.append(ln)
            else:
                out.append(ln)
        if where:
            return False
        path.write_text("\n".join(out), encoding="utf-8")
        return True

    def build(self, tag: str, picks: list[str], names: list[str]) -> dict:
        """OCCT and the named tests' executables at a state (only what
        changed since the state before is rebuilt)."""
        key = state_key(tag, picks)
        if self.current != key:
            self.current = None
            info = {"key": key, "occt": "ok", "tests": {}, "needs": [], "unprovided": []}
            self.api_picks = [p for p in picks if p.startswith(API_ONLY)]
            self.state_numbers = {pick_number(p) for p in picks}
            bad = self.prepare(tag, picks)
            if bad:
                info["occt"] = f"a carried patch does not apply at its place ({bad})"
                info["conflict"] = bad
                info["conflict_files"] = self.conflict_files
                return info
            if not build_occt(self.tree, self.work, self.args.build_jobs, self.out):
                errors = [ln for ln in last_occt_errors[0].splitlines() if "error:" in ln]
                if not errors or any(re.search(r"Killed signal|fatal error: Killed", ln) for ln in errors):
                    # Stopped from outside (memory, a signal): an error, run again on the next run.
                    info["occt"] = "error: the OCCT build stopped without a compiler error"
                else:
                    info["occt"] = "OCCT does not build at its place"
                    info["errors"] = errors[:20]
                return info
            sync_headers(self.tree, self.inc, self.headers)
            self.current = key
            self.info = info
        info = self.info
        todo = [n for n in names if n not in info["tests"]]
        if todo:
            self.build_tests(info, tag_index(self.nums, tag), todo)
        return info

    def build_tests(self, info: dict, base_idx: int, names: list[str]) -> None:
        targets = {n: test_target(self.tests[n], self.mbuild) for n in names}
        want = {t for t in targets.values() if t}
        if None in targets.values():
            want |= set(self.args.targets)
        env = dict(os.environ)
        env["CCACHE_BASEDIR"] = str(REPO)
        env["CCACHE_NOHASHDIR"] = "1"
        for _ in range(3):
            start = self.out.stat().st_size if self.out.exists() else 0
            run(["ninja", "-C", self.mbuild, "-k", "0", "-j", str(self.args.build_jobs), *sorted(want)],
                env=env, out=self.out, check=False)
            # A build that re-ran CMake (a changed glob, the port's files) re-ran
            # vcpkg install, which puts the tip's headers back: the state's
            # headers again, and the build again.
            if not sync_headers(self.tree, self.inc, self.headers):
                break
        with open(self.out, "rb") as f:
            f.seek(start)
            failed = failed_blocks(f.read().decode("utf-8", "replace"))
        if any(re.search(r"Killed signal|fatal error: Killed|signal 9", b) for b in failed.values()):
            # A compiler killed (memory) says nothing about the state: the item is an error, tried again.
            self.current = None
            raise RuntimeError(f"the MitCad build was killed ({self.out})")
        dirs = {exe_dir(t) for t in targets.values() if t}
        lib_failed = [o for o in failed if o.endswith(".o") and not any(o.startswith(d) for d in dirs)]
        relevant: set[str] = set() if all(targets.values()) else set(lib_failed)
        for n in names:
            t = targets[n]
            if t is None:
                ok = all(built(self.mbuild, x, failed) for x in self.args.targets)
                info["tests"][n] = {"build": "built" if ok else "failed", "exe": None,
                                    "failed": [Path(o).name.removesuffix(".o") for o in lib_failed][:5]}
                continue
            # The libraries' objects this executable needs that failed.
            inputs = target_inputs(self.mbuild, t)
            mine_lib = [o for o in lib_failed if o in inputs]
            status, exe, rel = self.exe_status(n, t, failed, mine_lib)
            relevant |= set(rel)
            info["tests"][n] = {"build": status, "exe": exe, "failed": [Path(o).name.removesuffix(".o") for o in rel][:5]}
        if relevant:
            needs, unprovided = self.providers("\n".join(failed[o] for o in relevant), base_idx)
            info["needs"] = sorted(set(info["needs"]) | needs, key=self.nums.index)
            info["unprovided"] = sorted(set(info["unprovided"]) | set(unprovided))[:20]

    def exe_status(self, name: str, target: str, failed: dict, lib_failed: list[str]):
        """('built' | 'partial' | 'failed', the executable to run in place of
        the ctest's, the failed edges that keep it from building)."""
        stubs = bool(self.api_picks)
        if built(self.mbuild, target, failed):
            if not stubs:
                return "built", None, []
            # Linked again so that OCCT's calls of functions only declared
            # at this state are bound too.
            exe = self.partial_link(target, name, [], True)
            return ("partial", exe, []) if exe else ("failed", None, [target])
        if lib_failed:
            return "failed", None, lib_failed
        # A function a later patch only declares here (carried as its
        # declarations) is bound to a stub that says so if it is called.
        if target in failed and not (stubs and "undefined reference" in failed[target]):
            return "failed", None, [target]
        if target in failed:
            exe = self.partial_link(target, name, [], stubs)
            return ("partial", exe, []) if exe else ("failed", None, [target])
        d = exe_dir(target)
        mine = [o for o in failed if o.startswith(d)]
        own = own_sources(self.tests[name])
        if own is None:
            return "failed", None, mine
        keep = [o for o in mine if o[len(d):].removesuffix(".o") in own]
        if keep or not mine:
            return "failed", None, keep
        exe = self.partial_link(target, name, mine, stubs)
        return ("partial", exe, []) if exe else ("failed", None, [])

    def partial_link(self, target: str, name: str, drop: list[str], any_undefined: bool = False) -> str | None:
        """The executable linked without the objects of the other parts that
        do not build here. Where the dispatcher's table of parts names their
        functions, each is bound to a stub that aborts (never called: only
        the part asked for runs). With any_undefined (later patches carried
        as their declarations only) every undefined reference is bound to
        it; the stub says so on its way out (STUB_CALLED)."""
        cmd = link_command(self.mbuild, target)
        if not cmd:
            return None
        folder = self.wdir / "partial"
        folder.mkdir(exist_ok=True)
        exe = folder / f"{Path(target).name}-{re.sub(r'[^\w.-]', '_', name)}"
        gone = set(drop)
        words = [w for w in cmd if w not in gone and not w.startswith("-Wl,--dependency-file=")]
        words[words.index("-o") + 1] = str(exe)
        # The state's OCCT libraries may call functions only declared at
        # this state (the build links against the installed tip's).
        libdir = occt_libs(self.tree)
        p = run(words + ["-Wl,--no-demangle"], cwd=self.mbuild, check=False)
        occt_missing = occt_undefined(libdir, [str(exe)] if p.returncode == 0 else []) if any_undefined else []
        if p.returncode == 0 and not occt_missing:
            return str(exe)
        text = p.stdout + p.stderr if p.returncode else ""
        missing = sorted(set(re.findall(r"undefined reference to [`‘']([^'’]+)['’]", text)) | set(occt_missing))
        if any_undefined and missing:
            for _ in range(3):
                if not self.stub_link(words, exe, missing):
                    return None
                more = set(occt_undefined(libdir, [str(exe)])) - set(missing)
                if not more:
                    return str(exe)
                missing = sorted(set(missing) | more)
            return None
        if not undefined_only_in_dispatchers(text):
            return None
        stub = folder / "missing-part.o"
        if not stub.exists():
            src = folder / "missing-part.cpp"
            src.write_text('// SPDX-License-Identifier: MIT\n#include <cstdlib>\n'
                           'extern "C" void occt_port_proof_missing_part() { std::abort(); }\n')
            if run([words[0], "-fPIC", "-c", src, "-o", stub], check=False).returncode != 0:
                return None
        defs = [f"-Wl,--defsym={sym}=occt_port_proof_missing_part" for sym in missing]
        p = run(words + [str(stub), *defs], cwd=self.mbuild, check=False)
        return str(exe) if p.returncode == 0 else None

    def stub_link(self, words: list[str], exe: Path, missing: list[str]) -> str | None:
        """The executable linked with a shared library of stubs, one function
        for each missing symbol (functions only declared at this state, also
        those OCCT's own libraries call), each writing STUB_CALLED and
        aborting. A missing vtable or type info (data) is not stubbed."""
        if any(re.match(r"_ZT[VIS]|vtable for|typeinfo", sym) for sym in missing) or \
                not all(re.fullmatch(r"\w+", sym) for sym in missing):
            return None
        key = hashlib.sha256(("named\n" + "\n".join(missing)).encode()).hexdigest()[:12]
        lib = exe.parent / f"libstubs-{key}.so"
        if not lib.exists():
            src = exe.parent / f"stubs-{key}.cpp"
            body = "".join(f'extern "C" void {sym}() {{ called("{sym}"); }}\n' for sym in missing)
            src.write_text('// SPDX-License-Identifier: MIT\n#include <cstdio>\n#include <cstdlib>\n'
                           'static void called(const char* sym) {\n'
                           f'  std::fprintf(stderr, "{STUB_CALLED}: %s\\n", sym);\n  std::abort();\n}}\n'
                           + body)
            if run([words[0], "-fPIC", "-shared", src, "-o", lib], check=False).returncode != 0:
                return None
        # Not as needed: OCCT's libraries, not the executable, may be the callers.
        p = run(words + ["-Wl,--no-as-needed", str(lib), f"-Wl,-rpath,{lib.parent}", "-Wl,--no-demangle"],
                cwd=self.mbuild, check=False)
        return str(exe) if p.returncode == 0 else None

    # -- the later patches whose API the MitCad code needs

    def pick_needs(self, base_idx: int, pick: str, files: list[str]) -> set[str]:
        """The patches between the base and a carried patch that wrote the
        lines its hunks change or stand beside in the files where its
        cherry-pick conflicts (git blame at its parent)."""
        owners: set[str] = set()
        for path in files:
            diff = git(self.tree, "diff", f"p-{pick}~1", f"p-{pick}", "--", path, check=False).stdout
            for m in re.finditer(r"^@@ -(\d+)(?:,(\d+))? ", diff, re.M):
                a, b = int(m.group(1)), int(m.group(2) or 1)
                if a == 0:
                    continue
                bl = git(self.tree, "blame", "--porcelain", "-L", f"{a},{a + max(b, 1) - 1}", f"p-{pick}~1", "--",
                         path, check=False).stdout
                owners |= set(re.findall(r"^([0-9a-f]{40}) \d+ \d+", bl, re.M))
        out: set[str] = set()
        lo, hi = base_idx, self.nums.index(pick)
        for sha in owners:
            subject = git(self.tree, "log", "-1", "--format=%s", sha, check=False).stdout.strip()
            n = number(subject) if subject.endswith(".patch") else None
            if n in self.nums and lo < self.nums.index(n) < hi:
                out.add(n)
        return out

    def header_added(self, n: str, cls: str = "") -> str:
        """The lines patch n adds to the headers (to the header of class cls only, if given)."""
        key = f"{n}:{cls}"
        if key not in self.added:
            globs = [f"*/{cls}.hxx", f"*/{cls}.lxx"] if cls else HEADER_GLOBS
            diff = git(self.tree, "diff", f"p-{n}~1", f"p-{n}", "--", *globs).stdout
            self.added[key] = "\n".join(ln[1:] for ln in diff.splitlines()
                                        if ln.startswith("+") and not ln.startswith("+++"))
        return self.added[key]

    def providers(self, text: str, base_idx: int) -> tuple[set[str], list[str]]:
        """The later patches adding the names and headers the errors miss
        (absent at the tree's state), and the names no later patch adds."""
        names: set[str] = set()
        # Names whose call matches no declaration there: an overload or a
        # parameter a later patch adds, so present at the state and still missing.
        overloads: set[str] = set()
        # Members a class lacks there ("'class C' has no member named 'X'",
        # "'X' is not a member of 'C'"): a later patch adding X to C's header,
        # though the name may be in other headers already (another class's).
        members: dict[str, str] = {}
        hdrs: set[str] = set()
        for ln in text.splitlines():
            m = (re.search(r"[‘'`](?:class |struct )?(\w+)[’'`] has no member named [‘'`](\w+)", ln)
                 or re.search(r"[‘'`](\w+)[’'`] is not a member of [‘'`](?:class |struct )?(\w+)", ln))
            if m:
                cls, nm = (m.group(1), m.group(2)) if "has no member" in ln else (m.group(2), m.group(1))
                members[nm] = cls.split("::")[-1]
            m = re.search(r"(\w+\.(?:hxx|lxx|gxx|h)):\d+:\d+: error: #error \"this header is not in this state", ln)
            if m:
                hdrs.add(m.group(1))
            elif "error:" in ln or "undefined reference" in ln:
                call = re.search(r"no matching function for call to|too many arguments|too few arguments", ln)
                for q in re.findall(r"[‘'`]([A-Za-z_][\w:]*)", ln):
                    nm = q.split("::")[-1]
                    if len(nm) > 2 and nm not in CPP_WORDS:
                        names.add(nm)
                        if call:
                            overloads.add(nm)
        needs: set[str] = set()
        unprovided: list[str] = []
        for h in sorted(hdrs):
            path = self.headers.get(h)
            subj = git(self.tree, "log", "--reverse", "--diff-filter=A", "--format=%s", "base..tip", "--",
                       path, check=False).stdout.split() if path else []
            n = number(subj[0]) if subj else None
            if n in self.nums and self.nums.index(n) > base_idx:
                needs.add(n)
            else:
                unprovided.append(h)
        for nm in sorted(names)[:80]:
            if nm not in overloads and nm not in members and git(
                    self.tree, "grep", "-q", "-w", "-F", "-e", nm, "HEAD", "--", *HEADER_GLOBS,
                    check=False).returncode == 0:
                continue
            pat = re.compile(rf"\b{re.escape(nm)}\b")
            later = [k for k in self.nums[base_idx + 1:] if k not in self.state_numbers]
            n = None
            if nm in members:
                n = next((k for k in later if pat.search(self.header_added(k, members[nm]))), None)
            n = n or next((k for k in later if pat.search(self.header_added(k))), None)
            if n:
                needs.add(n)
            else:
                unprovided.append(nm)
        return needs, unprovided

    # -- running

    def limit(self, name: str) -> float:
        base = self.plan["tip_seconds"].get(name) or self.args.max_timeout
        return min(self.args.max_timeout, max(self.args.min_timeout, self.args.timeout_factor * base))

    def command(self, name: str, ti: dict) -> dict:
        t = dict(self.tests[name])
        if ti.get("exe"):
            t["command"] = [ti["exe"], *t["command"][1:]]
        return t

    @staticmethod
    def needs_only(run_: dict, named: set[str]) -> bool:
        """Whether a failing run needs its named tests run one by one: in the
        chain a ctest often holds tests of later patches that fail at the
        patch's place, so both states are judged by the named tests alone."""
        if run_["status"] not in ("failed", "timeout", "stub called") or not named:
            return False
        return not named <= set(run_.get("only", {}))

    def visit(self, base_idx: int, wanted: list[str], names: list[str], run_names: list[str],
              named: set[str]) -> tuple[dict, str]:
        """A state's build (cached, or built now) and its runs of run_names."""
        tag, picks = chain_state(self.nums, base_idx, wanted)
        key = state_key(tag, picks)
        cached = self.cache.state(key)
        if cached and cached["occt"] != "ok" and not self.args.chain_retry:
            return cached, key
        if cached and all(n in cached["tests"] for n in names):
            pending = []
            for n in run_names:
                if cached["tests"][n]["build"] == "failed":
                    continue
                r = self.cache.run(key, n)
                if r is None or self.needs_only(r, named):
                    pending.append(n)
            if not pending:
                return cached, key
        info = self.build(tag, picks, names)
        if not info["occt"].startswith("error"):
            self.cache.put_state(info)
        if info["occt"] != "ok":
            return info, key
        libdir = occt_libs(self.tree)
        for n in run_names:
            ti = info["tests"][n]
            if ti["build"] == "failed":
                continue
            r = self.cache.run(key, n)
            fresh = r is None
            if fresh:
                r = run_ctest(self.command(n, ti), libdir, self.logs / f"{key}-{n}.log", self.limit(n))
                if ti["build"] == "partial":
                    r["partial"] = True
                    if r["status"] != "passed" and STUB_CALLED in Path(r["log"]).read_text(errors="replace"):
                        r["status"] = "stub called"
            if self.needs_only(r, named):
                # Each named test alone (MITCAD_TEST_ONLY), to see whether it is one that fails.
                r.setdefault("only", {})
                for fn in sorted(named - set(r["only"])):
                    lo = self.logs / f"{key}-{n}-{fn}.log"
                    o = run_ctest(self.command(n, ti), libdir, lo, self.limit(n),
                                  {"MITCAD_TEST_ONLY": only_selector(fn)})
                    if o["status"] != "passed" and STUB_CALLED in Path(o["log"]).read_text(errors="replace"):
                        o["status"] = "stub called"
                    r["only"][fn] = {"status": o["status"], "log": o["log"]}
                fresh = True
            if fresh:
                self.cache.put_run(key, n, r)
        return info, key

    def runs(self, key: str, info: dict, names: list[str]) -> dict:
        out = {}
        for n in names:
            ti = info["tests"].get(n, {"build": "failed", "failed": []})
            if ti["build"] == "failed":
                out[n] = {"status": "not built", "why": ", ".join(ti.get("failed", [])[:3])}
            else:
                out[n] = self.cache.run(key, n) or {"status": "-"}
        return out

    def item(self, item: str, names: list[str]) -> dict:
        """The patch at its place; where a test calls a function only declared
        there, the patch that defines it is carried whole and the patch shown
        again."""
        forced: set[str] = set()
        rec: dict = {}
        for _ in range(4):
            prev, rec = rec, self.attempt(item, names, forced)
            if prev and rec["state"] != "ran":
                # Carried whole, the patch does not apply or build there: the
                # run with its declarations stands.
                return prev
            called = self.stubs_called(rec)
            if rec["state"] != "ran" or not called:
                break
            hit = {pick_number(p) for p in rec["carried"] if p.startswith(API_ONLY) and any(
                re.search(rf"\b{re.escape(nm)}\b", self.header_added(pick_number(p))) for nm in called)}
            if not hit - forced:
                break
            forced |= hit
        return rec

    @staticmethod
    def stubs_called(rec: dict) -> set[str]:
        """The names of the functions whose stubs the item's runs called."""
        syms: set[str] = set()
        for runs in rec["runs"].values():
            for r in runs.values():
                logs = [r.get("log")] if r.get("status") == "stub called" else []
                logs += [o["log"] for o in r.get("only", {}).values() if o["status"] == "stub called"]
                for lg in logs:
                    text = Path(lg).read_text(errors="replace") if lg and Path(lg).exists() else ""
                    syms |= set(re.findall(re.escape(STUB_CALLED) + r": (\w+)", text))
        if not syms:
            return set()
        names = run(["c++filt", *sorted(syms)], check=False).stdout.split("\n")
        return {m.group(1) for n in names if (m := re.search(r"(\w+)\(", n))}

    def attempt(self, item: str, names: list[str], forced: set[str]) -> dict:
        group = item.split("+")
        base_idx = min(self.nums.index(n) for n in group) - 1
        named = named_functions(item)
        rec = {"item": item, "tests": names, "base": self.nums[base_idx] if base_idx >= 0 else "base",
               "carried": [], "state": "", "runs": {"without": {}, "with": {}},
               "date": datetime.date.today().isoformat()}
        # The later patches whose API the MitCad code needs, found state by
        # state: first their declarations alone (the changes to the headers),
        # so that no later code comes in; whole where those do not apply or
        # OCCT does not build with them.
        carried: list[str] = []
        order = pick_order(self.nums)

        def declared(n: str) -> str:
            return n if n in forced else API_ONLY + n

        def numbers(picks: list[str]) -> set[str]:
            return {pick_number(p) for p in picks}

        def whole(picks, more) -> list[str]:
            return sorted({p for p in picks if pick_number(p) not in more} | set(more), key=order)

        for _ in range(2 * self.args.max_carry + 2):
            info, _key = self.visit(base_idx, carried, names, [], named)
            if info["occt"] != "ok":
                bad = info.get("conflict")
                if bad and bad.startswith(API_ONLY):
                    carried = whole(carried, {pick_number(bad)})
                    continue
                if not bad:
                    api = {pick_number(p) for p in carried if p.startswith(API_ONLY)}
                    if api and info["occt"].startswith("OCCT does not build"):
                        # Declarations that use other later declarations: those too.
                        more, _ = self.providers("\n".join(info.get("errors", [])), base_idx)
                        more -= set(group) | numbers(carried)
                        if more and len(carried) + len(more) <= self.args.max_carry:
                            carried = sorted(set(carried) | {declared(n) for n in more}, key=order)
                            continue
                        carried = whole(carried, api)
                        continue
                    break
                # A carried patch that does not cherry-pick takes with it the
                # patches between whose lines its hunks build on.
                more = self.pick_needs(base_idx, bad, info.get("conflict_files", []))
                if more & set(group):
                    rec["entangled"] = [bad, *sorted(more & set(group))]
                    break
                new = more - numbers(carried)
                if not new or len(carried) + len(new) > self.args.max_carry:
                    if new:
                        rec["needs"] = sorted(numbers(carried) | new, key=self.nums.index)
                    break
                carried = whole(carried, new)
                continue
            need = set(info.get("needs", []))
            if need & set(group):
                rec["own_api"] = sorted(need & set(group))
            new = need - set(group) - numbers(carried)
            if not new:
                break
            if len(carried) + len(new) > self.args.max_carry:
                rec["needs"] = sorted(numbers(carried) | new, key=self.nums.index)
                break
            carried = sorted(set(carried) | {declared(n) for n in new}, key=order)
        rec["carried"] = carried
        if info.get("unprovided"):
            rec["unprovided"] = info["unprovided"]
        if rec.get("entangled"):
            rec["state"] = (f"the later patch {rec['entangled'][0]}, whose API the MitCad code needs, builds on "
                            f"this patch's lines, so it cannot be carried without it")
            return rec
        info, key = self.visit(base_idx, carried, names, names, named)
        if info["occt"] != "ok":
            rec["state"] = info["occt"] + " (before the patch)"
            return rec
        rec["runs"]["without"] = self.runs(key, info, names)
        info2, key2 = self.visit(base_idx, list(group) + carried, names, names, named)
        if info2["occt"] != "ok":
            rec["state"] = info2["occt"] + " (with the patch)"
            return rec
        rec["runs"]["with"] = self.runs(key2, info2, names)
        rec["states"] = [key, key2]
        rec["state"] = "ran"
        return rec


def chain_worker(args, wid: int) -> None:
    work = Path(args.work)
    w = ChainWorker(args, wid)
    tests = w.plan["tests"]
    while True:
        chunk = locked_pop(work / "chain-queue.json")
        if chunk is None:
            break
        for item in chunk:
            t0 = time.time()
            try:
                rec = w.item(item, tests[item])
            except Exception as exc:  # one item's failure is recorded, the worker goes on
                w.current = None
                with open(w.out, "a", encoding="utf-8") as f:
                    f.write(f"\n{item}: {traceback.format_exc()}")
                first = (str(exc).splitlines() or [""])[0][:200]
                rec = {"item": item, "tests": tests[item], "base": "-", "carried": [], "runs": {},
                       "state": f"error: {type(exc).__name__}: {first} (traceback in {w.out})"}
            rec["seconds"] = round(time.time() - t0)
            # What the result was made from, so that a later run knows whether it still holds.
            rec["stamp"] = w.plan["stamp"]
            append_result(work / "chain-results.jsonl", rec)
            v, _ = chain_verdict(rec)
            extra = f" (carried {carried_text(rec['carried'])})" if rec.get("carried") else ""
            log(f"c{wid}: {item}{extra}: {v}")


def table_items(table: Path, prefixes: list[str]) -> list[str]:
    """The items of the table whose verdict at the tip starts with one of the
    prefixes, as whole words ("fails with" is not "fails without")."""
    pats = [re.compile(re.escape(p) + r"(?![A-Za-z])") for p in prefixes]
    return [item for item, e in parse_table(table).items() if any(p.match(e["verdict"]) for p in pats)]


def chain_main(args, items: list[str], plan_tests: dict, order: list[str]) -> None:
    work = Path(args.work)
    nums = [number(f) for f in order]
    hist = make_history(work, find_tarball(args.tarball), order)
    log(f"history: {hist}")
    with_file = work / "with.json"
    tip_with = json.loads(with_file.read_text()) if with_file.exists() else {}
    stamp = chain_stamp(order)
    done = {i: r for i, r in load_chain_results(work).items() if r.get("stamp") == stamp}
    queue = [i for i in items
             if args.chain_retry or i in args.patches or i not in done or done[i]["state"].startswith("error")]
    queue.sort(key=lambda i: min(nums.index(n) for n in i.split("+")))
    jobs = args.jobs or args.workers
    # Runs of consecutive patches, each walked forwards by one worker; more
    # runs than workers so that a worker that finishes early takes another.
    size = args.chunk or max(1, -(-len(queue) // (2 * jobs)))
    chunks = [queue[k:k + size] for k in range(0, len(queue), size)]
    plan = {"order": order, "tests": plan_tests, "stamp": stamp, "date": datetime.date.today().isoformat(),
            "tip_seconds": {n: r.get("seconds") for n, r in tip_with.items() if isinstance(r, dict)}}
    (work / "chain-plan.json").write_text(json.dumps(plan, indent=1))
    (work / "chain-queue.json").write_text(json.dumps(chunks))
    log(f"in the chain: {len(queue)} patches ({len(items) - len(queue)} done before) in {len(chunks)} runs "
        f"on {min(jobs, len(chunks))} workers")
    me = [sys.executable, str(Path(__file__).resolve())]
    passthru = ["--work", str(work), "--preset", args.preset, "--build-jobs", str(args.build_jobs),
                "--max-carry", str(args.max_carry),
                "--timeout-factor", str(args.timeout_factor), "--min-timeout", str(args.min_timeout),
                "--max-timeout", str(args.max_timeout), "--tests-doc", args.tests_doc, "--targets", *args.targets]
    if args.chain_retry:
        passthru.append("--chain-retry")
    procs = []
    for wid in range(min(jobs, len(chunks))):
        cmd = shlex.split(args.wrap) + me + passthru + ["--chain-worker", str(wid)]
        procs.append(subprocess.Popen(cmd))
        time.sleep(2)
    rcs = [p.wait() for p in procs]
    tip_plan = json.loads((work / "plan.json").read_text()) if (work / "plan.json").exists() else {"with": tip_with}
    log(f"table: {write_table(args, tip_plan, load_results(work))}")
    if any(rcs):
        raise SystemExit(f"workers ended with {rcs}")


# ---------------------------------------------------------------- main


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("patches", nargs="*", help="patch numbers, or groups such as 0130+0140")
    ap.add_argument("--unproven", action="store_true",
                    help="every patch whose section in the test document says 'still to be shown'")
    ap.add_argument("--all", action="store_true", help="every patch whose Test paragraph names a ctest")
    ap.add_argument("--tests-doc", default=str(TESTS_DOC),
                    help="the test document: a section '## <number>' with a Test paragraph per patch")
    ap.add_argument("--work", default=str(REPO / "build" / "occt-proof"))
    ap.add_argument("--tarball", help=f"the OCCT source archive ({TARBALL})")
    ap.add_argument("--preset", default="dev", help="the MitCad configure preset (build/<preset>)")
    ap.add_argument("--targets", nargs="+", default=["geometry/all"], help="the MitCad targets the tests need")
    ap.add_argument("--workers", type=int, default=5)
    ap.add_argument("--build-jobs", type=int, default=6, help="compile jobs per worker")
    ap.add_argument("--wrap", default="", help="command prefix for each worker, e.g. a memory-capped scope")
    ap.add_argument("--max-extra", type=int, default=40,
                    help="the most later patches reverted with one (beyond, the patch is left unproven)")
    ap.add_argument("--timeout-factor", type=float, default=5.0, help="without-run limit: this times the with-run")
    ap.add_argument("--min-timeout", type=float, default=180.0)
    ap.add_argument("--max-timeout", type=float, default=1500.0)
    # Documents stay out of the port's own folder (the public gate's rule).
    ap.add_argument("--table", default=str(PORT.parent / "docs" / "opencascade-before-after.md"))
    ap.add_argument("--update-tests", "--update-headers", dest="update_tests", action="store_true",
                    help="rewrite the proof sentences in the test document from the last results "
                         "(of the patches given, or all); no .patch file changes")
    ap.add_argument("--list", action="store_true", help="only list the patches and their named ctests")
    ap.add_argument("--worker", type=int, help=argparse.SUPPRESS)
    ap.add_argument("--in-chain", action="store_true",
                    help="show each patch at its own place in the chain (before it / with it) instead of at the tip")
    ap.add_argument("--jobs", type=int, help="in the chain: workers, each its own OCCT tree and MitCad build "
                    "(default: --workers)")
    ap.add_argument("--chunk", type=int, help="in the chain: consecutive patches a worker takes at once "
                    "(default: the patches over twice the workers)")
    ap.add_argument("--max-carry", type=int, default=20,
                    help="in the chain: the most later patches carried for the API the MitCad code needs")
    ap.add_argument("--table-verdict", action="append", default=[], metavar="PREFIX",
                    help="also every patch whose verdict at the tip in --table starts with PREFIX (repeatable)")
    ap.add_argument("--chain-retry", action="store_true",
                    help="in the chain: take the given patches again, done ones too, and build again the states that did not build")
    ap.add_argument("--chain-worker", type=int, help=argparse.SUPPRESS)
    args = ap.parse_args()
    set_tests_doc(args.tests_doc)
    if args.worker is not None:
        worker(args, args.worker)
        return
    if args.chain_worker is not None:
        chain_worker(args, args.chain_worker)
        return
    work = Path(args.work)
    plan_file = work / "plan.json"

    if args.update_tests:
        # A run in the chain alone has no plan of the tip.
        plan = (json.loads(plan_file.read_text()) if plan_file.exists()
                else {"order": patch_order(), "with": {}, "date": datetime.date.today().isoformat()})
        results = load_results(work)
        changed = update_tests(args, plan, results)
        log(f"proof sentences rewritten in {tests_doc()}: {len(changed)} ({' '.join(changed)})")
        log(f"table: {write_table(args, plan, results)}")
        return

    order = patch_order()
    stray = stray_test_paragraphs(order)
    if stray:
        log(f"a Test paragraph in the header of {' '.join(stray)}: it belongs in {tests_doc()} "
            "(a header change rebuilds OCCT); this tool does not read it")
    if not tests_doc().exists():
        raise SystemExit(f"no test document {tests_doc()}")
    if not args.list:
        work.mkdir(parents=True, exist_ok=True)
    devbuild = REPO / "build" / args.preset
    known: set[str] | None = None
    tests: dict = {}
    if (devbuild / "build.ninja").exists():
        log(f"MitCad tests at the tip ({devbuild})")
        # In the chain each worker builds its own tests; the build only names them.
        if not args.list and not args.in_chain and not build_mitcad(devbuild, args.targets, args.build_jobs * 2,
                                                                    work / "dev-build.log"):
            raise SystemExit(f"the MitCad tests do not build ({work / 'dev-build.log'})")
        tests = ctest_table(devbuild)
        known = set(tests)
    elif args.list:
        log(f"{devbuild} is not configured: every ctest name a Test paragraph mentions is listed, unchecked")
    else:
        raise SystemExit(f"{devbuild} is not configured (cmake --preset {args.preset})")
    by_number = {number(f): f for f in order}
    no_section = [number(f) for f in order if not tests_of(number(f))]
    if no_section:
        log(f"no section in {tests_doc()} for {' '.join(no_section)}")

    items: list[str] = list(args.patches)
    for item in table_items(Path(args.table), args.table_verdict):
        if item not in items:
            items.append(item)
    if args.all or args.unproven:
        for f in order:
            text = tests_of(number(f))
            if named_ctests(text, known) and (args.all or unproven(text)):
                if number(f) not in items:
                    items.append(number(f))
    if not items:
        raise SystemExit("no patches given (numbers, --unproven, --all or --table-verdict)")
    plan_tests = {}
    for item in items:
        names: list[str] = []
        for n in item.split("+"):
            if n not in by_number:
                raise SystemExit(f"no patch {n} in portfile.cmake")
            for t in named_ctests(tests_of(n), known):
                if t not in names:
                    names.append(t)
        plan_tests[item] = names
    no_test = [i for i, t in plan_tests.items() if not t]
    if no_test:
        log(f"no ctest named in the section of {' '.join(no_test)} in {tests_doc()}: left out")
        for i in no_test:
            items.remove(i)
            del plan_tests[i]

    if args.list:
        for item in items:
            print(item, " ".join(plan_tests[item]))
        return
    if args.in_chain:
        chain_main(args, items, plan_tests, order)
        return
    hist = make_history(work, find_tarball(args.tarball), order)
    tip = work / "tip" / "src"
    (work / "tip").mkdir(exist_ok=True)
    sync_tree(tip, hist)
    log("OCCT at the tip")
    if not build_occt(tip, work, args.build_jobs * args.workers, work / "tip" / "build.log"):
        raise SystemExit(f"OCCT does not build at the tip ({work / 'tip' / 'build.log'})")
    libdir = occt_libs(tip)

    logs = work / "logs"
    logs.mkdir(exist_ok=True)
    with_file = work / "with.json"
    with_runs = json.loads(with_file.read_text()) if with_file.exists() else {}
    stamp = port_stamp(order)
    if with_runs.get("_stamp") != stamp:
        with_runs = {"_stamp": stamp}
    needed = sorted({t for ts in plan_tests.values() for t in ts if t not in with_runs})
    log(f"with: {len(needed)} ctests")
    from concurrent.futures import ThreadPoolExecutor

    def one(name):
        return name, run_ctest(tests[name], libdir, logs / f"with-{name}.log", args.max_timeout)

    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        for name, r in pool.map(one, needed):
            with_runs[name] = r
            if r["status"] != "passed":
                log(f"with: {name} {r['status']} ({r['why']})")
    with_file.write_text(json.dumps(with_runs, indent=1))

    plan = {"order": order, "tests": plan_tests, "with": with_runs, "date": datetime.date.today().isoformat()}
    plan_file.write_text(json.dumps(plan, indent=1))
    done = load_results(work)
    retry = {"ran", "test does not build", "does not revert"}
    queue = [i for i in items if i not in done or i in args.patches or done[i]["state"] not in retry]
    (work / "queue.json").write_text(json.dumps(queue))
    log(f"without: {len(queue)} patches on {args.workers} workers")
    me = [sys.executable, str(Path(__file__).resolve())]
    passthru = ["--work", str(work), "--preset", args.preset, "--build-jobs", str(args.build_jobs),
                "--max-extra", str(args.max_extra),
                "--timeout-factor", str(args.timeout_factor), "--min-timeout", str(args.min_timeout),
                "--max-timeout", str(args.max_timeout), "--tests-doc", args.tests_doc, "--targets", *args.targets]
    procs = []
    for wid in range(min(args.workers, len(queue))):
        cmd = shlex.split(args.wrap) + me + passthru + ["--worker", str(wid)]
        procs.append(subprocess.Popen(cmd))
        time.sleep(2)
    rcs = [p.wait() for p in procs]
    results = load_results(work)
    table = write_table(args, plan, results)
    log(f"table: {table}")
    if any(rcs):
        raise SystemExit(f"workers ended with {rcs}")


if __name__ == "__main__":
    main()
