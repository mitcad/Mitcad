#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Capture all 28 macOS parity groups without equating captures with review."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import struct
import subprocess
import sys
import time
import zlib


TOOLS = Path(__file__).resolve().parent
SOURCE = TOOLS.parent
# Keep these 28 original audit groups stable, including fixture/production
# distinctions. Extra states belong inside their original group.
GROUPS = (
    ("appearances", "Appearances", "production"),
    ("community", "Community Library", "production"),
    ("feedback", "Send Feedback (open only)", "production"),
    ("insert", "Insert from Library", "production"),
    ("libraries", "Libraries", "production"),
    ("new-project", "New Project", "production"),
    ("parameters", "Parameters, invalid Return/Tab and correction", "production"),
    ("parts-fixture", "Library Components fixture", "fixture"),
    ("picker-fixture", "Component Picker fixture and production BUILD options", "fixture+production"),
    ("project-settings", "Project Settings", "shared-entry-fixture"),
    ("projects", "Open a Project and selected Projects design + Return", "fixture+production"),
    ("publish", "Publish to Library", "production"),
    ("render", "Render Image", "production"),
    ("selection", "Main, selected face, Extrude and command search", "production"),
    ("settings-cache", "Settings Cache", "production"),
    ("settings-cloud", "Settings Cloud", "production"),
    ("settings-display", "Settings Display", "production"),
    ("settings-navigation", "Settings Navigation", "production"),
    ("settings-textures", "Settings Textures", "production"),
    ("shortcuts", "Keyboard Shortcuts", "production"),
    ("print", "Send to Slicer", "production"),
    ("profiles", "Processes (once Printers and Processes)", "production"),
    ("materials", "Materials", "production"),
    ("properties", "Properties", "production"),
    ("export", "Export", "production"),
    ("find", "Find in Design", "production"),
    ("overview", "Keyboard and Mouse Overview", "production"),
    ("parts-production", "Library Components production entry", "production"),
)
FAILURES = (
    "TestDriver: FAIL", "Unable to simultaneously satisfy constraints",
    "Native Mac canvas:", "OCCT view event failed", "Recompute failed",
)


def utc():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def artifact(path):
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha256(path)}


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    temporary.replace(path)


def quote(value):
    # TestDriver uses shell-style quoting, but no shell expansion.
    return shlex.quote(str(value))


class Scenario:
    def __init__(self, frontend, theme, directory):
        self.native = frontend == "native"
        self.theme = theme
        self.directory = directory
        self.lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
                      "expect-model-bodies 1 48000", "show-dialog-style-fixture setup"]
        self.images = []
        if self.native:
            self.add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
                     f"native-appearance {theme}")
        else:
            self.add("expect-ever 'View area'", "expect-never 'Native UI: AppKit shell'")

    def add(self, *lines):
        self.lines.extend(lines)

    def capture(self, state, dialog=True):
        for kind in ("content", "own-windows"):
            path = self.directory / f"{state}-{kind}.png"
            self.images.append({"state": state, "kind": kind, "path": str(path)})
            command = "screenshot-screen" if kind == "own-windows" else (
                "screenshot-dialog" if dialog else "screenshot")
            self.add(f"{command} {quote(path)}", f"expect-file {quote(path)}")

    def open(self, command, title):
        self.add(f"command {command}", f"focus-dialog {quote(title)}")
        if not self.native:
            self.add(f"expect-dialog-appearance {self.theme}")

    def close(self, title):
        self.add("key Escape", f"{'expect-native-dialog-gone' if self.native else 'expect-dialog-gone'} {quote(title)}",
                 "focus-main")


def parameter_states(s):
    s.open("solid.parameters", "^Parameters$")
    if s.native:
        s.add("native-dialog-size 1240 520", "expect-native-columns parametersTable fit")
    s.capture("parameters")
    for value, commit, state in (("30 deg", "Return", "invalid-return"), ("45 deg", "Tab", "invalid-tab"),
                                  ("25 mm", "Return", "corrected")):
        s.add("mark")
        if s.native:
            s.add("native-click parameter.d3.2", "expect-native-style parameter.d3.2 editing")
        else:
            s.add("click-parameter d3 2", "key F2")
        s.add("key Ctrl+a", f"type {quote(value)}", f"key {commit}",
              f"expect-log {quote('^Parameter d3 expression: ' + value + (' = ' if state == 'corrected' else ' refused:'))}",
              "wait-idle", "focus-dialog '^Parameters$' 2",
              f"expect-model-bodies 1 {60000 if state == 'corrected' else 48000}")
        if s.native:
            s.add(f"expect-native-text parameter.d3.2 {quote('^' + value + '$')}",
                  f"expect-native-text parameter.d3.3 {quote('^' + ('25' if state == 'corrected' else '20') + ' mm$')}",
                  f"expect-native-text parametersMessage {quote('^$' if state == 'corrected' else '^d3: .*expected length')}")
            if commit == "Tab":
                s.add("expect-native-style parameter.d3.4 editing", "key Escape")
            s.add("native-select parameter.d1.0",
                  f"expect-native-style parameter.d3.2 {'normal' if state == 'corrected' else 'error'},not-editing")
        else:
            s.add(f"expect-parameter-cell d3 2 {quote('^' + value + '$')} {'visible' if state == 'corrected' else 'refused'}",
                  f"expect-parameter-cell d3 3 {quote('^' + ('25' if state == 'corrected' else '20') + ' mm$')} visible")
        s.capture(state)
    s.add("button '^OK$'", "focus-main", "expect-model-bodies 1 60000")


def build_picker(s):
    # A fixture capture is kept separately from the production command path.
    s.add("show-dialog-style-fixture picker", "focus-dialog '^Component Picker'")
    s.capture("picker-fixture")
    s.add("button '^Close$'", "focus-main", "mark", "command build.profile",
          "expect-log 'Command Profile started'")
    s.add("native-pointer-click input_part" if s.native else "click-logged 'Panel Profile input part at '")
    s.add("expect-log 'Part picker opened: 2 parts for build.profile'", "focus-dialog '^Component Picker'")
    s.capture("build-picker")
    if s.native:
        s.add("expect-native-tiles pictures partPickerGrid 2",
              "native-tiles click partPickerGrid 'Profile 30 test'",
              "expect-native-visible partOption0 true", "expect-native-visible partOption1 true",
              "expect-native-text partOption0 '^A$'", "native-pointer-choice partOption0 B Down Return",
              "expect-log '^Native pointer choice: partOption0 .*menu tracked$'",
              "expect-native-text partOption0 '^B$'", "native-key-event Tab",
              "expect-native-control-focus partOption1")
    else:
        s.add("dialog-click-logged 'Part picker tile test-profiles/p30-test.* at '",
              "qt-choice partOption0 B", "focus-widget partOption1")
    s.add("expect-log 'Part picker option Grade chosen: B'",
          "expect-log 'Part picker option moves: Offset 8 mm.*5 mm: Grade B offset'",
          "mark", "key Ctrl+a", "type 4", "native-key-event Tab" if s.native else "key Tab",
          "expect-log 'Part picker option Offset chosen: 4 mm'",
          "expect-log 'Part picker chose test-profiles/p30-test.*with options Grade = B, Offset = 4 mm'",
          "expect-log '^Inspect Profile: place grounded, length 1000.00 mm'",
          "expect-never '^Inspect Profile: failed:'")
    s.capture("build-picker-options")
    s.add("button '^Close$'", "focus-main", "key Escape", "expect-log 'Command Profile cancelled'",
          "expect-model-bodies 1 48000")


def projects(s):
    s.add("wait-log 'Projects panel: 1 project' 30")
    if s.native:
        s.add("native-expand projectsTree", "native-select projects.row.Audit/base.mitcad",
              "expect-native-focus projectsTree",
              "expect-native-selection projectsTree projects.row.Audit/base.mitcad")
    else:
        # Expand the real project row, then Return with its design selected
        # and the actual tree holding keyboard focus.
        s.add("focus-widget projectsTree", "key Home", "key Right",
              "wait-log 'Projects row Audit/base.mitcad at ' 30",
              "click-logged 'Projects row Audit/base.mitcad at '", "focus-widget projectsTree")
    s.add("mark", "key Return", "expect-log 'Projects panel: open Audit/base.mitcad'",
          "wait-log '^Opened .*/Audit/base\\.mitcad$' 30", "wait-idle", "expect-model-bodies 0 0")
    s.capture("projects-return", dialog=False)
    s.add("show-dialog-style-fixture projects", "focus-dialog '^Open a Project$'")
    s.capture("open-project-fixture")
    s.close("^Open a Project$")


def selection(s):
    s.add("command view.fit", "sleep 600")
    s.capture("main", dialog=False)
    s.add("click-visible-face", "expect-selection 1 0",
          "expect-log '^Selection card create: Extrude \\| Create Sketch \\| Hole$'")
    if s.native:
        s.add("expect-native-selection-card 3")
    s.capture("selected-face", dialog=False)
    s.add("command solid.extrude", "expect-log 'Command Extrude started'")
    s.capture("extrude", dialog=False)
    s.add("key Escape", "expect-log 'Command Extrude cancelled'", "focus-main", "command tools.search")
    if s.native:
        s.add("native-type native.commandSearch 'Library Sources'",
              "expect-native-visible native.commandSearch true",
              "expect-native-text native.commandSearch '^Library Sources$'",
              "expect-native-style native.command.tools.libraries row-selected,in-view")
    else:
        s.add("focus-widget commandSearchInput", "type 'Library Sources'",
              "expect-command-search 'Library Sources' tools.libraries")
    s.capture("command-search", dialog=False)
    s.add("key Escape", "focus-main", "expect-model-bodies 1 48000")


def make_scenario(group, frontend, theme, directory):
    s = Scenario(frontend, theme, directory)
    dialogs = {
        "appearances": ("solid.appearances", "^Appearances$"),
        "community": ("tools.community_library", "^Community Library$"),
        "feedback": ("help.send_feedback", "^Send Feedback$"),
        "insert": ("insert.library_component", "^Insert from Library$"),
        "libraries": ("tools.libraries", "^Library Sources$"),
        "new-project": ("file.new_project", "^New Project$"),
        "publish": ("tools.publish_library", "^Publish to Library$"),
        "render": ("file.render_image", "^Render Image$"),
        "shortcuts": ("tools.shortcuts", "^Keyboard Shortcuts$"),
        # The Processes manager, the richer of the two since mitcad#1135.
        "profiles": ("make.processes", "^Processes$"),
        "materials": ("solid.materials", "^Materials$"),
        "properties": ("inspect.properties", "^Properties$"),
        "export": ("file.export", "^Export$"),
        "find": ("view.find_in_design", "^Find in Design$"),
        "overview": ("help.shortcuts", "^Keyboard and Mouse Overview$"),
    }
    if group in dialogs:
        command, title = dialogs[group]
        s.open(command, title)
        if group == "new-project" and s.native:
            s.add("expect-native-form-layout 'projectFolder=Folder:' 'projectDisplayName=Display name:' 'projectAuthorName=Author:' 'projectAuthorEmail=Author:'")
        if group in ("community", "insert") and s.native:
            s.add("expect-native-tiles pictures libraryResults 2", "expect-native-tiles captions libraryResults")
        if group == "properties":
            s.add("native-dialog-resize 620 480" if s.native else "dialog-resize 620 480")
            if s.native:
                s.add("expect-native-text propertiesBody0_heading '^Body1$'",
                      "expect-native-text propertiesTotal_heading '^Total$'",
                      "expect-native-text propertiesBody0_volume '^48000 mm³$'",
                      "expect-native-text propertiesTotal_volume '^48000 mm³$'",
                      "expect-native-form-layout 'propertiesBody0_volume=Volume' 'propertiesBody0_area=Area' 'propertiesBody0_center=Center of Mass' 'propertiesTotal_volume=Volume'",
                      "expect-native-style propertiesBody0_heading bold",
                      "expect-native-style propertiesTotal_heading bold",
                      "expect-native-style propertiesBody0_volume regular,readonly,selectable",
                      "native-pointer-click propertiesBody0_volume", "key Ctrl+a",
                      "expect-native-style propertiesBody0_volume selected")
            else:
                s.add("expect-widget-property propertiesBody0_volume text '48000 mm³'",
                      "expect-widget-property propertiesTotal_volume text '48000 mm³'",
                      "focus-widget propertiesBody0_volume", "key Ctrl+a",
                      "expect-widget-property propertiesBody0_volume selectedText '48000 mm³'")
            # Exercise selection without replacing the user's clipboard;
            # leave an unselected value for the comparison captures.
            s.add("key Right", "key Tab", "expect-model-bodies 1 48000")
        s.capture(group)
        if group in ("appearances", "new-project", "shortcuts"):
            width, height = (400, 400) if group == "new-project" else (720, 440)
            s.add(f"native-dialog-resize {width} {height}" if s.native else f"dialog-resize {width} {height}")
            if group == "appearances" and s.native:
                s.add("expect-log '^Native dialog size: requested 720x440, actual 720x440$'",
                      "expect-native-bounds appearanceBody 0",
                      "expect-native-bounds appearanceTextureEmbed 100",
                      "expect-native-bounds appearanceSearch 100",
                      "expect-native-bounds appearanceClose 30",
                      "expect-native-form-layout appearanceBody appearanceClose")
            s.capture(group + "-narrow")
        s.close(title)
    elif group.startswith("settings-"):
        s.open("tools.preferences." + group.removeprefix("settings-"), "^Settings$")
        if group == "settings-cloud" and s.native:
            s.add("expect-native-bounds cloudAuthorNote shared", "expect-native-bounds cloudGitStatus shared")
        s.capture(group)
        s.add("key Ctrl+w", f"{'expect-native-dialog-gone' if s.native else 'expect-dialog-gone'} '^Settings$'", "focus-main")
    elif group == "parameters":
        parameter_states(s)
    elif group == "parts-fixture":
        s.add("show-dialog-style-fixture parts", "focus-dialog '^Components in This Design$'")
        s.capture("parts-fixture")
        s.close("^Components in This Design$")
    elif group == "picker-fixture":
        build_picker(s)
    elif group == "project-settings":
        s.add("show-dialog-style-fixture project-settings", "focus-dialog '^Project Settings'")
        s.capture(group)
        s.close("^Project Settings")
    elif group == "projects":
        projects(s)
    elif group == "selection":
        selection(s)
    elif group == "print":
        s.add("command make.print3d", "expect-log 'Command Send to Slicer started'")
        s.capture(group, dialog=False)
        s.add("key Escape", "expect-log 'Command Send to Slicer cancelled'")
    elif group == "parts-production":
        s.open("insert.library_component", "^Insert from Library$")
        if s.native:
            s.add("native-tiles dblclick libraryResults iso4762")
        else:
            # LibraryBrowser owns a PartGrid directly, so it does not emit
            # the modeless PartPicker's pointer-coordinate diagnostics.
            # Activate the actual first grid item through its keyboard path.
            s.add("focus-widget libraryResults", "key Home",
                  "expect-ever '^Library browser selected mitcad-parts/iso4762:'",
                  "key Return")
        s.add("expect-log 'Inserted component .* from mitcad-parts'", "wait-idle", "focus-main")
        s.open("tools.library_parts", "^Components in This Design$")
        s.capture(group)
        s.close("^Components in This Design$")
    else:
        raise ValueError(group)
    # Brackets prevent the command's own log echo matching whole-log checks.
    s.add("expect-never 'Native Mac canvas[:]|OCCT view event failed|Recompute failed'")
    return s


def shell_runner(app, cli, output, script, library, cache, frontend, theme, timeout, prepare_project):
    exports = {
        "MITCAD_TEST_INPUT": script, "MITCAD_TEST_COLOR_SCHEME": theme,
        "MITCAD_TEST_WORK": output, "MITCAD_STYLE_LIBRARY": library,
        "MITCAD_LIBRARIES_DIR": cache, "MITCAD_UI_REQUIRE_GUI": "1",
    }
    lines = ["#!/usr/bin/env bash", "# SPDX-License-Identifier: MIT", "set -euo pipefail",
             f". {shlex.quote(str(TOOLS / 'ui-macos-lib.sh'))}",
             f"ui_init {shlex.quote(str(app))} {shlex.quote(str(output))}", f"UI_TIMEOUT={timeout}"]
    lines.extend(f"export {key}={shlex.quote(str(value))}" for key, value in exports.items())
    if prepare_project:
        lines.append(f"{shlex.quote(str(cli))} project create \"$MITCAD_PROJECTS_DIR/Audit\" "
                     f"--author 'UI Tester <ui@example.invalid>' --design base.mitcad > {shlex.quote(str(output / 'project-setup.log'))} 2>&1")
    # Qt is deliberately the default startup path in its own QtOFF binary.
    args = ["--demo", "--no-recovery", "--no-mcp"]
    if frontend == "native":
        args.insert(0, "--ui=native")
    lines += ["ui_run_app capture " + " ".join(map(shlex.quote, args)),
              "ui_expect_log 'TestDriver: done' 'surface script completed'",
              "ui_expect_no_log 'TestDriver: FAIL' 'surface assertions passed'",
              '[ "$UI_FAILURES" = 0 ]']
    return "\n".join(lines) + "\n", args


def png_metadata(path):
    data = path.read_bytes()
    if len(data) < 33 or data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise ValueError("missing PNG IHDR")
    width, height = struct.unpack(">II", data[16:24])
    if width < 100 or height < 100 or width > 32768 or height > 32768:
        raise ValueError(f"implausible capture size {width}x{height}")
    offset = 8
    ended = False
    while offset + 12 <= len(data):
        length = struct.unpack(">I", data[offset:offset + 4])[0]
        end = offset + 12 + length
        if end > len(data):
            raise ValueError("incomplete PNG chunk")
        kind = data[offset + 4:offset + 8]
        checksum = struct.unpack(">I", data[end - 4:end])[0]
        if zlib.crc32(data[offset + 4:end - 4]) & 0xFFFFFFFF != checksum:
            raise ValueError("PNG chunk checksum mismatch")
        if kind == b"IEND":
            ended = True
            break
        offset = end
    if not ended:
        raise ValueError("missing PNG end chunk")
    return {**artifact(path), "width": width, "height": height}


def source_metadata():
    result = {}
    try:
        result["checkout_revision"] = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=SOURCE, text=True).strip()
        result["checkout_status"] = subprocess.check_output(
            ["git", "status", "--porcelain"], cwd=SOURCE, text=True)
        files = subprocess.check_output(["git", "ls-files", "-z"], cwd=SOURCE).split(b"\0")
        manifest = hashlib.sha256()
        count = 0
        for name in sorted(filter(None, files)):
            path = SOURCE / os.fsdecode(name)
            if path.is_file():
                manifest.update(name + b"\0" + sha256(path).encode() + b"\n")
                count += 1
        result.update(tracked_files=count, tracked_content_sha256=manifest.hexdigest())
    except (OSError, subprocess.CalledProcessError) as error:
        result["source_manifest_error"] = str(error)
    return result


def prepare(args, output):
    environment = os.environ.copy()
    environment.update(MITCAD_SETTINGS_DIR=str(output / "setup-settings"),
                       MITCAD_LIBRARIES_DIR=str(output / "library-cache"),
                       MITCAD_PROJECTS_DIR=str(output / "setup-projects"),
                       MITCAD_PRINT_LIBRARY_DIR=str(output / "setup-print-library"),
                       MITCAD_AUTOSAVE_DIR=str(output / "setup-autosave"),
                       MITCAD_THUMBNAIL_DIR=str(output / "setup-thumbnails"),
                       GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=str(output / "empty.gitconfig"))
    (output / "empty.gitconfig").write_text("", encoding="utf-8")
    fasteners = output / "fasteners"
    profiles = output / "profile-fixture"
    commands = [
        [sys.executable, str(TOOLS / "libraries/make-fastener-library.py"), "--cli", str(args.cli),
         "--out", str(fasteners), "--sizes", "M5", "--standards", "iso4762,iso4032", "--version", "1.0.0",
         "--no-render", "--commit"],
        ["bash", str(TOOLS / "ui-macos-native-part-picker-test.sh"), str(args.app), str(args.cli), str(profiles), "--prepare-only"],
        [str(args.cli), "library", "fetch", str(fasteners), "--libraries", str(output / "library-cache"), "--json"],
    ]
    records = []
    for index, command in enumerate(commands):
        log = output / f"setup-{index + 1}.log"
        started = utc()
        with log.open("w", encoding="utf-8") as stream:
            result = subprocess.run(command, env=environment, stdout=stream, stderr=subprocess.STDOUT, check=False)
        records.append({"argv": command, "started": started, "finished": utc(), "exit_code": result.returncode,
                        "log": artifact(log)})
        write_json(output / "setup.json", records)
        if result.returncode:
            raise RuntimeError(f"fixture setup failed: {log}")


def report_markdown(report):
    lines = ["# macOS parity capture evidence", "", f"Source revision supplied: `{report['source_revision']}`.",
             f"Frontend: {report['frontend']}; execution: {report['execution_status']}.", "",
             "Capture completion does not establish visual or interaction acceptance.", "",
             "| Group | Theme | Execution | Visual review | Interaction review |", "| --- | --- | --- | --- | --- |"]
    for run in report["runs"]:
        folder = f"{run['theme']}/{run['group']}"
        lines.append(f"| [{run['group']}]({folder}) | {run['theme']} | {run['execution_status']} | pending | pending |")
    lines += ["", "Manual acceptance remains pending:", "", "- Maintainer acceptance of this integrated iteration.",
              "- Physical trackpad input.", "- VoiceOver and accessibility navigation.",
              "- Oldest supported macOS runtime.", "", "Record reviewed findings separately with image and source references."]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", required=True, type=Path)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--frontend", choices=("qt", "native"), required=True)
    parser.add_argument("--revision", required=True, help="Full source commit used to build this binary")
    parser.add_argument("--output", required=True, type=Path, help="A new evidence directory; never reused")
    parser.add_argument("--build-evidence", action="append", default=[], type=Path,
                        help="Build manifest, CMake cache or warning-free build log retained with this report")
    parser.add_argument("--only", help="Comma-separated group IDs for an explicitly incomplete diagnostic run")
    parser.add_argument("--plan-only", action="store_true", help="Write all scripts and planned records without running CLI or GUI")
    parser.add_argument("--timeout", type=int, default=180, help="Seconds per application run")
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-fA-F]{40}", args.revision):
        parser.error("--revision must be a full 40-digit source commit")
    if args.timeout < 30:
        parser.error("--timeout must be at least 30 seconds")
    selected = set(args.only.split(",")) if args.only else {group[0] for group in GROUPS}
    if not selected or selected - {group[0] for group in GROUPS}:
        parser.error("--only contains an unknown or empty group ID")
    args.app = args.app.resolve(strict=True)
    args.cli = args.cli.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    dependencies = [Path(__file__), TOOLS / "ui-macos-lib.sh", TOOLS / "ui-macos-native-part-picker-test.sh",
                    TOOLS / "libraries/make-fastener-library.py"]
    report = {
        "schema": 1, "started": utc(), "source_revision": args.revision, "source": source_metadata(),
        "frontend": args.frontend, "comparison": "Current candidate Qt default vs explicit AppKit, same source; not a released artifact comparison",
        "app": artifact(args.app), "cli": artifact(args.cli), "scripts": [artifact(path.resolve()) for path in dependencies],
        "build_evidence": [],
        "binary_source_mapping": "pending independent verification against retained build manifest",
        "host": {"system": platform.system(), "macos": platform.mac_ver()[0], "machine": platform.machine()},
        "complete_inventory": not bool(args.only), "expected_groups": len(GROUPS), "expected_runs": len(GROUPS) * 2,
        "execution_status": "planned", "visual_review": "pending", "interaction_review": "pending",
        "manual_checks": {key: {"status": "pending", "reason": "Requires explicit human or runtime evidence"}
                          for key in ("maintainer", "physical_trackpad", "voiceover", "oldest_macos")}, "runs": [],
    }
    for index, path in enumerate(args.build_evidence):
        original = path.resolve(strict=True)
        retained = output / "build-evidence" / f"{index + 1}-{original.name}"
        retained.parent.mkdir(exist_ok=True)
        shutil.copyfile(original, retained)
        report["build_evidence"].append({"original": artifact(original), "retained": artifact(retained)})
    for theme in ("light", "dark"):
        for group, title, entry in GROUPS:
            directory = output / theme / group
            directory.mkdir(parents=True)
            scenario = make_scenario(group, args.frontend, theme, directory)
            script = directory / "capture.mitcad-ui"
            script.write_text("\n".join(scenario.lines) + "\n", encoding="utf-8")
            library = output / ("profile-fixture/test-profiles" if group == "picker-fixture" else "fasteners")
            cache = output / ("profile-fixture/library-cache" if group == "picker-fixture" else "library-cache")
            shell, launch = shell_runner(args.app, args.cli, directory, script, library, cache,
                                         args.frontend, theme, args.timeout, group == "projects")
            runner = directory / "run.sh"
            runner.write_text(shell, encoding="utf-8")
            report["runs"].append({"group": group, "title": title, "entry": entry, "theme": theme,
                                   "selected": group in selected, "execution_status": "planned" if group in selected else "not-selected",
                                   "visual_review": "pending", "interaction_review": "pending", "launch_arguments": launch,
                                   "script": artifact(script), "runner": artifact(runner), "images": scenario.images})
    report_path = output / "report.json"
    write_json(report_path, report)
    try:
        if not args.plan_only:
            prepare(args, output)
            report["execution_status"] = "running"
            write_json(report_path, report)
            for run in report["runs"]:
                if not run["selected"]:
                    continue
                directory = Path(run["runner"]["path"]).parent
                print(f"Capturing {args.frontend}/{run['theme']}/{run['group']}", flush=True)
                started = time.monotonic()
                run["started"] = utc()
                run["execution_status"] = "running"
                write_json(report_path, report)
                with (directory / "harness.log").open("w", encoding="utf-8") as log:
                    result = subprocess.run(["bash", run["runner"]["path"]], stdout=log, stderr=subprocess.STDOUT, check=False)
                run.update(finished=utc(), elapsed_seconds=round(time.monotonic() - started, 3), exit_code=result.returncode)
                app_log = directory / "capture.log"
                text = app_log.read_text(encoding="utf-8", errors="replace") if app_log.exists() else ""
                # Assertions echo their patterns in TestDriver trace lines;
                # diagnose actual messages, not the assertions themselves.
                messages = "\n".join(line for line in text.splitlines() if not line.startswith("TestDriver: line "))
                errors = [message for message in FAILURES if message in messages]
                if result.returncode:
                    errors.append(f"harness exit {result.returncode}")
                if "TestDriver: done" not in text:
                    errors.append("driver did not complete")
                if args.frontend == "native" and "Native UI: AppKit shell" not in messages:
                    errors.append("native shell was not confirmed")
                if args.frontend == "qt" and "Native UI: AppKit shell" in messages:
                    errors.append("Qt default launch unexpectedly used native shell")
                for capture in run["images"]:
                    try:
                        capture.update(png_metadata(Path(capture["path"])))
                    except (OSError, ValueError) as error:
                        errors.append(f"{capture['state']}/{capture['kind']}: {error}")
                run["logs"] = [artifact(path) for path in (app_log, directory / "harness.log") if path.exists()]
                run["errors"] = errors
                run["execution_status"] = "failed" if errors else "captured"
                write_json(report_path, report)
                if errors:
                    # Preserve the first failure and unrun inventory. A new
                    # invocation gets a new evidence directory, never a retry
                    # that overwrites the failed scenario's pictures or logs.
                    raise RuntimeError(f"{run['theme']}/{run['group']}: " + "; ".join(errors))
            for original in [report["app"], report["cli"], *report["scripts"]]:
                if sha256(Path(original["path"])) != original["sha256"]:
                    raise RuntimeError(f"input changed during captures: {original['path']}")
            report["execution_status"] = "incomplete" if args.only else "captured"
    except (OSError, RuntimeError, KeyboardInterrupt) as error:
        report["execution_status"] = "failed"
        report["error"] = str(error)
        print(f"FAIL: {error}", file=sys.stderr)
    finally:
        report["finished"] = utc()
        write_json(report_path, report)
        (output / "report.md").write_text(report_markdown(report), encoding="utf-8")
    print(f"Evidence: {report_path}; visual and interaction review pending", flush=True)
    return 1 if report["execution_status"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
