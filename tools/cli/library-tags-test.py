# SPDX-License-Identifier: MIT
"""A library version is tagged once (mitcad#1372): with --commit, each
library generator (make-parts-library.py, make-fastener-library.py,
make-profile-library.py) refuses a --version whose tag v<version> its
--out repository already has, with a message naming the tag, before it
makes anything: the clone keeps its files, its commit and the tag where
they were. library_tags.py itself: a tag by its exact name, a folder that
is no repository of its own (also one inside a repository with the tag)
has none. Needs git and Python, not mitcad-cli: the generators stop before
they would run it.

    python3 library-tags-test.py --libraries <tools/libraries> --work <folder> [--git <git>]
"""

import argparse
import importlib.util
import os
import shutil
import subprocess
import sys

FAILURES = []
VERSION = "2.11.0"
GENERATORS = ["make-parts-library.py", "make-fastener-library.py", "make-profile-library.py"]


def check(condition, message):
    if not condition:
        FAILURES.append(message)


def git(executable, folder, *arguments):
    result = subprocess.run([executable, "-C", folder, "-c", "user.name=Library Test",
                             "-c", "user.email=test@example.invalid"] + list(arguments),
                            capture_output=True, text=True)
    if result.returncode != 0:
        sys.exit(f"library tags: git {' '.join(arguments)} failed:\n{result.stdout}{result.stderr}")
    return result.stdout.strip()


def load_tags(libraries):
    spec = importlib.util.spec_from_file_location("library_tags", os.path.join(libraries, "library_tags.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def state(executable, clone):
    """What a refused run must leave as it was: the files, the commit, the
    tag's commit and the tags."""
    files = sorted(os.path.relpath(os.path.join(root, name), clone).replace(os.sep, "/")
                   for root, dirs, names in os.walk(clone) if ".git" not in root.split(os.sep)
                   for name in names)
    return {"files": files, "head": git(executable, clone, "rev-parse", "HEAD"),
            "tag": git(executable, clone, "rev-parse", f"v{VERSION}^{{commit}}"),
            "tags": git(executable, clone, "tag", "--list"),
            "status": git(executable, clone, "status", "--porcelain")}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--libraries", required=True, help="tools/libraries")
    parser.add_argument("--work", required=True, help="a folder of the build tree (emptied first)")
    parser.add_argument("--git", default="git")
    args = parser.parse_args()
    libraries = os.path.abspath(args.libraries)
    work = os.path.abspath(args.work)
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    # The user's and the system's git settings (signing, hooks) stay out.
    config = os.path.join(work, "gitconfig")
    with open(config, "w", encoding="utf-8") as f:
        f.write("")
    os.environ["GIT_CONFIG_GLOBAL"] = config
    os.environ["GIT_CONFIG_NOSYSTEM"] = "1"
    os.environ["PYTHONDONTWRITEBYTECODE"] = "1"

    # A clone of a published library: one commit, tagged v2.11.0.
    clone = os.path.join(work, "parts")
    os.makedirs(clone)
    git(args.git, work, "init", "-q", "-b", "main", clone)
    with open(os.path.join(clone, "README.md"), "w", encoding="utf-8") as f:
        f.write("# A published library\n")
    git(args.git, clone, "add", "-A")
    git(args.git, clone, "commit", "-q", "-m", f"Library {VERSION}")
    git(args.git, clone, "tag", "-a", f"v{VERSION}", "-m", f"Library {VERSION}")
    os.makedirs(os.path.join(work, "supplier"))

    # library_tags: the exact tag, and only a repository of its own.
    tags = load_tags(libraries)
    check(tags.tag_of(VERSION) == "v2.11.0", f"the tag of {VERSION} is {tags.tag_of(VERSION)}")
    check(tags.tag_exists(clone, VERSION), "the clone's tag v2.11.0 not found")
    check(not tags.tag_exists(clone, "2.12.0"), "a tag v2.12.0 found in the clone, which has none")
    check(not tags.tag_exists(clone, "2.11"), "v2.11 taken for the tag v2.11.0")
    inside = os.path.join(clone, "inside")
    os.makedirs(inside)
    check(not tags.tag_exists(inside, VERSION), "a folder inside the clone judged by the clone's tags")
    check(not tags.tag_exists(os.path.join(work, "missing"), VERSION), "a missing folder has a tag")
    os.rmdir(inside)

    # Each generator with --commit and the clone's version: refused, naming
    # the tag, and nothing made. --cli names no program: a generator that
    # went on would fail later with another message.
    before = state(args.git, clone)
    for generator in GENERATORS:
        command = [sys.executable, os.path.join(libraries, generator), "--cli", os.path.join(work, "no-cli"),
                   "--out", clone, "--work", work, "--version", VERSION, "--no-render", "--commit"]
        if generator != "make-fastener-library.py":
            command += ["--source", os.path.join(work, "supplier")]
        result = subprocess.run(command, capture_output=True, text=True)
        output = result.stdout + result.stderr
        check(result.returncode != 0, f"{generator}: version {VERSION} with --commit not refused:\n{output}")
        check(f"already has the tag v{VERSION}" in output,
              f"{generator}: the refusal does not name the tag v{VERSION}:\n{output}")
        check("Traceback" not in output, f"{generator}: a traceback, not a refusal:\n{output}")
        after = state(args.git, clone)
        for key, value in before.items():
            check(after[key] == value, f"{generator}: the clone's {key} changed: {value!r} -> {after[key]!r}")
    if FAILURES:
        sys.exit("library tags: FAILED:\n" + "\n".join(FAILURES))
    print("library tags: ok")


if __name__ == "__main__":
    main()
