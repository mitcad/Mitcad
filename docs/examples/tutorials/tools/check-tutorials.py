#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Checks a MitCad tutorials repository against the package format
(https://github.com/mitcad/Mitcad/blob/main/docs/tutorials.md): the
repository's manifest mitcad-tutorials.json, and for every folder in
tutorials/ its manifest tutorial.json, its pages and designs, its files.
MitCad leaves a tutorial with a wrong manifest out of its list; this check
says why before a pull request is merged.

Usage: check-tutorials.py [<the repository's folder>]
The folder defaults to the one above this script's. Prints each problem as
"<path>: <what is wrong>" and exits with 1 when there is one.
Python 3.9 or newer, the standard library only.
"""
import json
import re
import sys
from pathlib import Path

REPOSITORY_MANIFEST = "mitcad-tutorials.json"
TUTORIALS = "tutorials"
TUTORIAL_MANIFEST = "tutorial.json"
# What MitCad reads (docs/tutorials.md, "A tutorials repository").
MAX_MANIFEST_BYTES = 4 << 20
MAX_FILES = 2000
MAX_TUTORIAL_BYTES = 100 << 20
MAX_REPOSITORY_BYTES = 200 << 20
# A byte order mark before a manifest's JSON is allowed.
BOM = chr(0xFEFF)

REPOSITORY_FIELDS = {"format": str, "version": int, "name": str, "description": str}
TUTORIAL_FIELDS = {"id": str, "title": str, "language": str, "version": str, "min_mitcad": str,
                   "license": str, "description": str, "start": str, "designs": list,
                   "duration": int, "level": str, "picture": str, "order": int}
REQUIRED = ("id", "title", "language", "version", "min_mitcad", "license")
# What the Start page's card shows when a tutorial gives it.
LEVELS = ("beginner", "intermediate", "advanced")
PICTURE_TYPES = (".png", ".jpg", ".jpeg")
MAX_PICTURE_BYTES = 4 << 20

ID = re.compile(r"^[a-z0-9][a-z0-9-]{0,63}$")
LANGUAGE = re.compile(r"^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$")
VERSION = re.compile(r"^[0-9]+(\.[0-9]+)*(-[0-9A-Za-z.]+)?$")
LICENSE = re.compile(r"^[A-Za-z0-9.+-]+( (AND|OR|WITH) [A-Za-z0-9.+-]+)*$")


class Problems:
    def __init__(self, root):
        self.root = root
        self.lines = []

    def add(self, path, text):
        where = path.relative_to(self.root).as_posix() if path != self.root else "."
        self.lines.append("%s: %s" % (where, text))


def read_manifest(path, problems):
    """The JSON object of a manifest, or None with the problem noted."""
    if not path.is_file():
        problems.add(path, "missing")
        return None
    data = path.read_bytes()
    if len(data) > MAX_MANIFEST_BYTES:
        problems.add(path, "%d bytes, more than the %d a manifest may have" % (len(data), MAX_MANIFEST_BYTES))
        return None
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        problems.add(path, "not UTF-8 text")
        return None
    try:
        value = json.loads(text[1:] if text.startswith(BOM) else text)
    except json.JSONDecodeError as error:
        problems.add(path, "not JSON: %s" % error)
        return None
    if not isinstance(value, dict):
        problems.add(path, "not a JSON object")
        return None
    return value


def check_fields(path, manifest, fields, problems):
    """Every field known and of its type; a misspelt field would be left
    out by MitCad without a word."""
    for name, value in manifest.items():
        kind = fields.get(name)
        if kind is None:
            problems.add(path, "unknown field \"%s\"" % name)
        elif kind is int and (isinstance(value, bool) or not isinstance(value, int)):
            problems.add(path, "%s is not a whole number" % name)
        elif kind is not int and not isinstance(value, kind):
            problems.add(path, "%s is not a %s" % (name, "list" if kind is list else "text"))


def valid_path(path):
    """A path inside the tutorial's folder, as MitCad accepts it."""
    return (bool(path) and not path.startswith("/") and "\\" not in path and ":" not in path
            and all(part not in ("", ".", "..") and part.lower() != ".git" for part in path.split("/")))


def check_repository(root, problems):
    path = root / REPOSITORY_MANIFEST
    manifest = read_manifest(path, problems)
    if manifest is None:
        return
    check_fields(path, manifest, REPOSITORY_FIELDS, problems)
    if manifest.get("format") != "mitcad-tutorials" or manifest.get("version") != 1:
        problems.add(path, "format must be \"mitcad-tutorials\" and version 1")
    if not str(manifest.get("name", "")).strip():
        problems.add(path, "name is missing")


def check_files(folder, problems):
    """No links, at most MAX_FILES files and MAX_TUTORIAL_BYTES bytes."""
    count = 0
    size = 0
    for path in sorted(folder.rglob("*")):
        if path.is_symlink():
            problems.add(path, "a symbolic link; tutorials hold files")
        elif path.is_file():
            count += 1
            size += path.stat().st_size
        if path.name.lower() == ".git":
            problems.add(path, "a git folder or submodule; tutorials hold files")
    if count > MAX_FILES or size > MAX_TUTORIAL_BYTES:
        problems.add(folder, "%d files and %d bytes; a tutorial has at most %d files and %d MB"
                     % (count, size, MAX_FILES, MAX_TUTORIAL_BYTES >> 20))


def check_tutorial(folder, problems):
    path = folder / TUTORIAL_MANIFEST
    manifest = read_manifest(path, problems)
    check_files(folder, problems)
    if manifest is None:
        return
    check_fields(path, manifest, TUTORIAL_FIELDS, problems)
    for name in REQUIRED:
        if not str(manifest.get(name, "")).strip():
            problems.add(path, "%s is missing" % name)
    tutorial_id = manifest.get("id", "")
    if isinstance(tutorial_id, str) and tutorial_id:
        if not ID.match(tutorial_id):
            problems.add(path, "id \"%s\" is not lower case letters, digits and hyphens (at most 64)" % tutorial_id)
        elif tutorial_id != folder.name:
            problems.add(path, "id \"%s\" is not its folder's name \"%s\"" % (tutorial_id, folder.name))
    for name, pattern, what in (("language", LANGUAGE, "a language code such as en or pt-BR"),
                                ("version", VERSION, "a version such as 1.0"),
                                ("min_mitcad", VERSION, "a MitCad version such as 0.68.0"),
                                ("license", LICENSE, "an SPDX licence id such as CC-BY-4.0")):
        value = manifest.get(name)
        if isinstance(value, str) and value.strip() and not pattern.match(value):
            problems.add(path, "%s \"%s\" is not %s" % (name, value, what))
    start = manifest.get("start") or "index.html"
    designs = manifest.get("designs", [])
    pages = [start] if isinstance(start, str) else []
    if isinstance(designs, list):
        if not all(isinstance(design, str) for design in designs):
            problems.add(path, "designs is not a list of paths")
        pages += [design for design in designs if isinstance(design, str)]
    for page in pages:
        if not valid_path(page):
            problems.add(path, "%s is not a path inside the tutorial" % page)
        elif not (folder / page).is_file():
            problems.add(path, "%s is not a file of the tutorial" % page)
    level = manifest.get("level")
    if isinstance(level, str) and level and level not in LEVELS:
        problems.add(path, "level \"%s\" is not one of %s" % (level, ", ".join(LEVELS)))
    for name in ("duration", "order"):
        value = manifest.get(name)
        if isinstance(value, int) and not isinstance(value, bool) and value < 1:
            problems.add(path, "%s is %d; it starts at 1" % (name, value))
    picture = manifest.get("picture")
    if isinstance(picture, str) and picture:
        if not valid_path(picture):
            problems.add(path, "%s is not a path inside the tutorial" % picture)
        elif not picture.lower().endswith(PICTURE_TYPES):
            problems.add(path, "picture %s is not a PNG or JPEG file" % picture)
        elif not (folder / picture).is_file():
            problems.add(path, "%s is not a file of the tutorial" % picture)
        elif (folder / picture).stat().st_size > MAX_PICTURE_BYTES:
            problems.add(path, "picture %s is larger than %d MB" % (picture, MAX_PICTURE_BYTES >> 20))


def check(root):
    """The problems of the repository at `root`, one line each."""
    root = Path(root).resolve()
    problems = Problems(root)
    check_repository(root, problems)
    tutorials = root / TUTORIALS
    folders = []
    if not tutorials.is_dir():
        problems.add(root, "no %s folder" % TUTORIALS)
    else:
        for entry in sorted(tutorials.iterdir()):
            if entry.is_dir() and not entry.is_symlink():
                folders.append(entry)
                check_tutorial(entry, problems)
            else:
                problems.add(entry, "not a tutorial's folder; %s holds one folder per tutorial" % TUTORIALS)
        if not folders:
            problems.add(tutorials, "no tutorials")
    size = sum(path.stat().st_size for path in root.rglob("*")
               if path.is_file() and not path.is_symlink() and ".git" not in path.relative_to(root).parts)
    if size > MAX_REPOSITORY_BYTES:
        problems.add(root, "%d bytes; a tutorials repository is fetched up to %d MB"
                     % (size, MAX_REPOSITORY_BYTES >> 20))
    return folders, problems.lines


def main(arguments):
    if len(arguments) > 1:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    root = arguments[0] if arguments else Path(__file__).resolve().parent.parent
    folders, problems = check(root)
    for line in problems:
        print(line)
    if problems:
        print("%d problem(s) in %d tutorial(s)" % (len(problems), len(folders)), file=sys.stderr)
        return 1
    print("%d tutorial(s) checked: no problems" % len(folders))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
