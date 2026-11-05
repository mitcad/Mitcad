# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build app/framework core/model/src/build core/model-build core/model-api/src tools/libraries
"""Timber seating through the actual Qt GUI and localhost MCP bridge.

Run only in the approved macOS VM:
  python3 ui-timber-seating-test.py <app> <cli> <output>
The production library generator makes synthetic screw/nail/board parts.
The public Wall and Swap Fastener UI commands place them; measurements
come from the placed bodies, not the requested seat metadata. No external
library, CAD file, server or desktop automation permission is needed.
"""

import base64
import importlib.util
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile


def support_module():
    spec = importlib.util.spec_from_file_location(
        "timber_mcp_support", Path(__file__).with_name("ui-macos-mcp-test.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MCP = support_module()
MCP.TIMEOUT = 120
TOLERANCE = 1e-6


def near(actual, expected, description):
    MCP.check(isinstance(actual, (int, float)) and abs(actual - expected) <= TOLERANCE,
              f"{description}: {actual!r}, expected {expected} within {TOLERANCE} mm")


def run(app, cli, out):
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="seating-", dir=out) as directory:
        work = Path(directory)
        source = Path(__file__).resolve().parent.parent
        env = dict(os.environ)
        for key in ("EMAIL", "GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL",
                    "MITCAD_TEST_INPUT", "MITCAD_TEST_RECOMPUTE_DELAY_MS"):
            env.pop(key, None)
        for key, folder in (("MITCAD_SETTINGS_DIR", "settings"), ("MITCAD_AUTOSAVE_DIR", "autosave"),
                            ("MITCAD_PROJECTS_DIR", "projects"), ("MITCAD_THUMBNAIL_DIR", "thumbnails"),
                            ("MITCAD_CRASH_DIR", "crashes"), ("MITCAD_MCP_DISCOVERY_DIR", "discovery"),
                            ("MITCAD_LIBRARIES_DIR", "libraries")):
            env[key] = str(work / folder)
        env.update(MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1",
                   MITCAD_TIMBER_WORK=str(work), GIT_CONFIG_NOSYSTEM="1")
        config = work / "gitconfig"
        config.write_text("", encoding="utf-8")
        env["GIT_CONFIG_GLOBAL"] = str(config)
        settings = work / "settings" / "nocodo.fi"
        settings.mkdir(parents=True)
        (settings / "Mitcad.ini").write_text("[autosave]\nenabled=false\n", encoding="utf-8")
        supplier = work / "supplier"
        supplier.mkdir()
        library = work / "parts"
        with (out / "fixture.log").open("wb") as log:
            subprocess.run([sys.executable, str(source / "tools/libraries/make-profile-library.py"),
                            "--cli", cli, "--source", str(supplier), "--out", str(library),
                            "--groups", str(source / "tools/cli/tests/timber-groups"), "--work", str(work),
                            "--version", "1.0.0", "--no-render", "--commit"],
                           env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=240)
            subprocess.run([cli, "library", "fetch", str(library)], env=env,
                           stdout=log, stderr=subprocess.STDOUT, check=True, timeout=60)
        script = "expect-ever 'View area'\nexpect-file '${MITCAD_TIMBER_WORK}/finish' 300\nquit\n"
        gui = MCP.Gui(app, env, out, "timber-seating", script, "qt")
        bridge = None
        try:
            discovery = Path(env["MITCAD_MCP_DISCOVERY_DIR"])
            MCP.wait_for(lambda: discovery.exists() and bool(list(discovery.glob("*.json"))),
                         "Timber GUI endpoint", gui)
            bridge = MCP.Bridge(cli, env, out, "timber-seating-bridge")
            bridge.initialize()
            document = bridge.tool("document_list", {})["documents"][0]["document"]

            def query(value):
                return bridge.tool("model_query", {"document": document, "query": value})["result"]

            def ui(inputs=None, identifier=None, finish="keep_open"):
                request = {"document": document, "finish": finish}
                if inputs is not None:
                    request["inputs"] = inputs
                if identifier:
                    request["id"] = identifier
                return bridge.tool("ui_command", request)

            # Inputs are supplied in stages, as a user starts Wall (the
            # TIMBER group's, mitcad#520), chooses a part, then the
            # Fasteners fields.
            ui({"part": "mitcad-parts|test-board|22x100"}, "build.wall")
            ui({"frame_fasteners": True})
            ui({"fastener": "mitcad-parts|test-screw"})
            made = ui({"fastener_size": "5.0x50", "fastener_count": "2",
                       "frame_length": "1200 mm", "frame_height": "600 mm", "frame_spacing": "600 mm"},
                      finish="ok")
            MCP.check(made.get("committed") is True, "Wall did not commit through its UI: " + repr(made))

            def inspect(label, tip, expected_config):
                parts = query({"query": "build_parts"})["parts"]
                fasteners = [part for part in parts if part["role"] == "fastener"]
                boards = [part for part in parts if part.get("kind") == "timber"]
                MCP.check(len(boards) == 5 and len(fasteners) == 12,
                          f"{label}: expected five boards and twelve fasteners: {parts}")
                for part in fasteners:
                    MCP.check(part["config"] == expected_config, f"{label}: wrong fastener row: {part}")
                    seating = part.get("seating")
                    MCP.check(isinstance(seating, dict), f"{label}: no physical seating measurement: {part}")
                    near(seating["head_top_distance"], 0.0, label + " " + part["component"] + " head top")
                    near(seating["tip_distance"], tip, label + " " + part["component"] + " tip")
                for part in boards:
                    MCP.check(not part["cuts"] and not part["machining"],
                              label + ": seating cut a board: " + repr(part))
                (out / (label + "-parts.json")).write_text(json.dumps(parts, indent=2), encoding="utf-8")
                print(f"ok   {label}: all {len(fasteners)} head tops flush within 1e-6 mm, tips {tip} mm")
                return fasteners, boards

            screws, boards = inspect("screw", 50.0, "5.0x50")
            bodies = query({"query": "bodies", "properties": True})
            board_bodies = {part["part"] + ".b0": None for part in boards}
            for body in bodies:
                if body["uid"] in board_bodies:
                    board_bodies[body["uid"]] = body
            MCP.check(all(value is not None for value in board_bodies.values()), "A board body is missing")
            log = bridge.tool("ui_log", {"limit": 2000})
            seating_lines = [line["text"] for line in log["lines"]
                             if line["text"].startswith("Timber fastener ")]
            MCP.check(len(seating_lines) == 12 and all(
                text.endswith("head top 0 mm from entry face") for text in seating_lines),
                "The actual Wall UI did not log twelve flush placements: " + repr(seating_lines))

            def paths(nodes, prefix=""):
                found = {}
                for node in nodes:
                    path = prefix + node["uid"]
                    found[node["component"]] = path
                    found.update(paths(node.get("children", []), path + "/"))
                return found

            occurrences = paths(query({"query": "components"})["occurrences"])
            bridge.tool("ui_select", {"document": document, "items": [
                {"kind": "body", "owner": part["part"] + ".b0",
                 "occurrence": occurrences[part["component"]]} for part in screws]})
            ui({"part": "mitcad-parts|test-nail"}, "build.swap_fastener")
            swapped = ui({"fastener_size": "4.0x40"}, finish="ok")
            MCP.check(swapped.get("committed") is True, "Swap Fastener did not commit through its UI")
            nails, _ = inspect("nail", 41.5, "4.0x40")
            MCP.check([part["component"] for part in nails] == [part["component"] for part in screws],
                      "Swap changed the fastener components")
            for body in query({"query": "bodies", "properties": True}):
                if body["uid"] in board_bodies:
                    MCP.check(body == board_bodies[body["uid"]], "Swap changed a board's physical body")
            new_log = bridge.tool("ui_log", {"since": log["lines"][-1]["seq"], "limit": 2000})
            new_lines = [line["text"] for line in new_log["lines"]
                         if line["text"].startswith("Timber fastener ")]
            MCP.check(len(new_lines) == 12 and all(
                text.endswith("head top 0 mm from entry face") for text in new_lines),
                "The actual Swap UI did not log twelve flush placements: " + repr(new_lines))
            bridge.tool("ui_camera", {"frame": "shown"})
            screenshot = bridge.tool("ui_screenshot", {"window": True, "max_size": 1600})
            MCP.check(screenshot["image"]["mimeType"] == "image/png", "Screenshot image block has the wrong MIME type")
            image = base64.b64decode(screenshot["image"]["data"], validate=True)
            MCP.check(len(image) >= 24 and image.startswith(b"\x89PNG\r\n\x1a\n"),
                      "The actual Qt window did not produce a PNG")
            width, height = struct.unpack(">II", image[16:24])
            MCP.check((width, height) == (screenshot["width"], screenshot["height"]),
                      "Screenshot metadata and actual PNG dimensions disagree")
            MCP.check(len(image) == screenshot["bytes"] and max(width, height) <= 1600,
                      "Screenshot byte count or size limit is incorrect")
            (out / "timber-seating.png").write_bytes(image)
            bridge.finish()
            (work / "finish").touch()
            gui.process.wait(timeout=30)
            MCP.check(gui.process.returncode == 0 and "TestDriver: FAIL" not in gui.log_text(), gui.log_text())
            print("PASS: UI Timber seating test (Qt)")
        finally:
            if bridge:
                bridge.stop()
            gui.stop()


def main():
    if len(sys.argv) != 4:
        raise SystemExit("usage: ui-timber-seating-test.py <app> <cli> <output>")
    if sys.platform != "darwin":
        raise SystemExit("This acceptance test runs in the approved macOS VM")
    session = subprocess.run(["launchctl", "managername"], capture_output=True, text=True)
    if session.stdout.strip() == "System" and os.environ.get("MITCAD_UI_REQUIRE_GUI") != "1":
        print("SKIP: no window server in the System session")
        raise SystemExit(77)
    run(str(Path(sys.argv[1]).resolve()), str(Path(sys.argv[2]).resolve()), Path(sys.argv[3]).resolve())


if __name__ == "__main__":
    main()
