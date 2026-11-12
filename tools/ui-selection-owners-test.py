# SPDX-License-Identifier: MIT
# check-all sources: app/framework core/model core/ffi tools/cli
"""Selection owners and bounded face gaps in both Qt layouts, through MCP.

Run in the approved platform VM (Linux under Xvfb):
  python3 ui-selection-owners-test.py <app> <cli> <output> [qt-bin]
The TestDriver reads the visible QLabel, checks its plain text and saves
screenshots. MCP selects named OCCT entities through the public UI API.
"""

import importlib.util
import os
from pathlib import Path
import struct
import sys
import tempfile

# python3 -I (the Linux wrappers) leaves this script's folder off the
# path; ui_macos_lock lives beside it.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ui_macos_lock import gui_lock


def support_module():
    spec = importlib.util.spec_from_file_location(
        "selection_mcp_support", Path(__file__).with_name("ui-macos-mcp-test.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MCP = support_module()


def run(app, cli, out, qt_bin):
    with gui_lock() as gui_options:
        _run_locked(app, cli, out, qt_bin, gui_options)


def _run_locked(app, cli, out, qt_bin, gui_options):
    out.mkdir(parents=True, exist_ok=True)
    for chrome in ("docked", "floating"):
        with tempfile.TemporaryDirectory(prefix=chrome + "-", dir=out) as directory:
            work = Path(directory)
            env = dict(os.environ)
            env.pop("MITCAD_TEST_INPUT", None)
            for key, folder in (("MITCAD_SETTINGS_DIR", "settings"), ("MITCAD_AUTOSAVE_DIR", "autosave"),
                                ("MITCAD_PROJECTS_DIR", "projects"), ("MITCAD_THUMBNAIL_DIR", "thumbnails"),
                                ("MITCAD_CRASH_DIR", "crashes"), ("MITCAD_MCP_DISCOVERY_DIR", "discovery")):
                env[key] = str(work / folder)
            env.update(MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_NEW_DESIGN_VIEW="0", MITCAD_LANGUAGE="en", MITCAD_NO_UPDATE_CHECK="1",
                       QT_FORCE_STDERR_LOGGING="1", MITCAD_SELECTION_WORK=str(work))
            if qt_bin:
                env["PATH"] = qt_bin + os.pathsep + env.get("PATH", "")
            # Each selected block must name its own owners before its measurements.
            nested = r"Component: Bench:1 › Screw M8:1\nBody: Screw Body"
            cases = [
                ("root-edge", r"(?s)\A(?!.*Component:).*Body: Root Plate\nLength:"),
                ("nested-edge", "(?s)" + nested + r"\nLength:"),
                ("nested-face", "(?s)" + nested + r"\nArea:"),
                ("nested-body", "(?s)" + nested + r"\nArea:.*\nVolume:"),
                ("mixed", "(?s)" + nested + r"\nLength:.*Body: Root Plate\nLength:"),
                ("pair-parallel", r"(?s)Body: Gap Base\nArea:.*Body: Gap Plate\nArea:.*Between faces\nDistance: 5 mm"),
                ("pair-angled", r"(?s)Body: Gap Base\nArea:.*Body: Gap Plate\nArea:.*Minimum distance: 5 mm\nMaximum distance: 17 mm"),
                ("pair-moved", r"(?s)Minimum distance: 8 mm\nMaximum distance: 20 mm"),
                ("pair-touching", r"(?s)Minimum distance: 0 mm\nMaximum distance: 12 mm"),
                ("pair-unit", r"(?s)Minimum distance: 0\.5 cm\nMaximum distance: 1\.7 cm"),
                ("pair-unavailable", r"(?s)\A(?!.*Maximum distance: [0-9]).*Minimum distance:.*Maximum distance: unavailable"),
                ("pair-single", r"(?s)\A(?!.*Between faces).*Body: Gap Base\nArea:"),
                ("pair-cleared", r"(?s)\A(?!.*Between faces)(?!.*Body:)"),
            ]
            script = "expect-ever 'View area'\n"
            script += "expect-file '${MITCAD_SELECTION_WORK}/framed' 60\nwait-idle 30\n"
            for name, expected in cases:
                script += "expect-file '${MITCAD_SELECTION_WORK}/" + name + "' 60\n"
                script += "expect-selection-info '" + expected + "'\n"
                script += "screenshot '${MITCAD_SELECTION_WORK}/" + name + ".png'\n"
            script += "expect-file '${MITCAD_SELECTION_WORK}/finish' 60\nquit\n"
            step_count = len(script.splitlines())
            gui = MCP.Gui(app, env, out, chrome, script, "qt", ["--chrome=" + chrome], gui_options=gui_options)
            bridge = None
            try:
                discovery = Path(env["MITCAD_MCP_DISCOVERY_DIR"])
                MCP.wait_for(lambda: discovery.exists() and bool(list(discovery.glob("*.json"))),
                             "selection GUI endpoint", gui)
                bridge = MCP.Bridge(cli, env, out, chrome + "-bridge")
                bridge.initialize()
                document = bridge.tool("document_list", {})["documents"][0]["document"]

                def command(value):
                    bridge.tool("model_command", {"document": document, "command": value})

                def query(value):
                    return bridge.tool("model_query", {"document": document, "query": value})["result"]

                # A fresh GUI document has an active Component1 occurrence. The
                # root-body case must explicitly create geometry in C0.
                command({"cmd": "activate_component", "component": "C0"})
                command(MCP.BOX)
                root = query({"query": "bodies"})[0]["uid"]
                command({"cmd": "rename_body", "uid": root, "name": "Root Plate"})
                command({"cmd": "create_component", "name": "Bench"})
                command({"cmd": "create_component", "name": "Screw M8"})
                command(MCP.BOX)
                child = next(body["uid"] for body in query({"query": "bodies"}) if body["uid"] != root)
                command({"cmd": "rename_body", "uid": child, "name": "Screw Body"})
                command({"cmd": "activate_component", "component": "C0"})
                components = query({"query": "components"})
                bench = next(occurrence for occurrence in components["occurrences"]
                             if occurrence["name"] == "Bench:1")
                screw = next(occurrence for occurrence in bench["children"]
                             if occurrence["name"] == "Screw M8:1")
                occurrence = bench["uid"] + "/" + screw["uid"]
                MCP.check(bench["name"] == "Bench:1" and screw["name"] == "Screw M8:1",
                          "The fixture does not contain the expected nested occurrences")

                def entity(kind, body, placed=""):
                    item = {"kind": kind, "owner": body, "occurrence": placed}
                    if kind != "body":
                        entries = query({"query": {"edge": "edges", "face": "faces"}[kind],
                                         "body": body})
                        item["name"] = entries[0]["names"][0] if kind == "face" else entries[0]["name"]
                    return item

                root_edge = entity("edge", root)
                edge = entity("edge", child, occurrence)
                selections = [[root_edge], [edge], [entity("face", child, occurrence)],
                              [entity("body", child, occurrence)],
                              [edge, root_edge]]

                def plate(name, translation):
                    previous = {body["uid"] for body in query({"query": "bodies"})}
                    command({"cmd": "create_component", "name": name,
                             "transform": {"translation": translation}})
                    command({"cmd": "add_feature", "def": {
                        "type": "box", "plane": "xy", "corner": [0, 0],
                        "length": 20, "width": 20, "height": 1, "operation": "new_body"}})
                    body = next(body["uid"] for body in query({"query": "bodies"})
                                if body["uid"] not in previous)
                    command({"cmd": "rename_body", "uid": body, "name": name})
                    command({"cmd": "activate_component", "component": "C0"})
                    placed = next(row["uid"] for row in query({"query": "components"})["occurrences"]
                                  if row["name"] == name + ":1")
                    face = next(name for row in query({"query": "faces", "body": body})
                                for name in row["names"] if name.endswith(":bottom"))
                    return {"kind": "face", "owner": body, "name": face, "occurrence": placed}

                base = plate("Gap Base", [50, 10, 0])
                other = plate("Gap Plate", [50, 10, 5])
                before_cylinder = {body["uid"] for body in query({"query": "bodies"})}
                command({"cmd": "add_feature", "def": {
                    "type": "cylinder", "plane": "xy", "center": [90, 10],
                    "diameter": 4, "height": 10, "operation": "new_body"}})
                cylinder = next(body["uid"] for body in query({"query": "bodies"})
                                if body["uid"] not in before_cylinder)
                round_face = next(name for row in query({"query": "faces", "body": cylinder})
                                  for name in row["names"] if name.endswith(":side0"))
                curved = {"kind": "face", "owner": cylinder, "name": round_face, "occurrence": ""}
                selections += [[base, other]] * 5 + [[base, curved], [base], []]
                # view.* MCP replies wait for the camera animation to end.
                # Frame the complete synthetic scene, then release the driver.
                bridge.tool("ui_command", {"document": document, "id": "view.fit"})
                (work / "framed").touch()

                def move_plate(height):
                    command({"cmd": "set_occurrence_transform", "occurrence": other["occurrence"],
                             "transform": {"translation": [50, 10, height],
                                           "rotation": {"axis": [0, 1, 0], "angle": -0.6435011087932844}}})

                for index, ((name, _), selected) in enumerate(zip(cases, selections)):
                    if name in ("pair-angled", "pair-moved", "pair-touching", "pair-unit"):
                        move_plate({"pair-angled": 5, "pair-moved": 8,
                                    "pair-touching": 0, "pair-unit": 5}[name])
                    if name == "pair-unit":
                        command({"cmd": "set_units", "length": "cm"})
                    elif name == "pair-unavailable":
                        command({"cmd": "set_units", "length": "mm"})
                    bridge.tool("ui_select", {"document": document, "items": selected})
                    (work / name).touch()
                    screenshot = work / (name + ".png")
                    # QImage opens the file before encoding is complete. The
                    # following driver gate proves that saving has returned.
                    following = cases[index + 1][0] if index + 1 < len(cases) else "finish"
                    marker = ": expect-file '${MITCAD_SELECTION_WORK}/" + following + "'"
                    MCP.wait_for(lambda: marker in gui.log_text(),
                                 chrome + " " + name + " selection screenshot saved", gui)
                    image = screenshot.read_bytes()
                    MCP.check(image[:8] == b"\x89PNG\r\n\x1a\n", "Selection screenshot is not PNG")
                    width, height = struct.unpack(">II", image[16:24])
                    MCP.check(width >= 300 and height >= 200, "Selection screenshot is too small")
                    (out / (chrome + "-" + name + ".png")).write_bytes(image)
                bridge.finish()
                (work / "finish").touch()
                gui.process.wait(timeout=30)
                completed = "TestDriver: quit (%d of %d steps)" % (step_count, step_count)
                MCP.check(gui.process.returncode == 0 and completed in gui.log_text()
                          and "TestDriver: FAIL" not in gui.log_text(), gui.log_text())
                print("PASS:", chrome, "selection owners, placed parallel/angled face gaps, refresh, units, unavailable and clearing")
            finally:
                if bridge is not None:
                    bridge.stop()
                gui.stop()


if __name__ == "__main__":
    if len(sys.argv) not in (4, 5):
        raise SystemExit("usage: ui-selection-owners-test.py <app> <cli> <output> [qt-bin]")
    run(str(Path(sys.argv[1]).resolve()), str(Path(sys.argv[2]).resolve()),
        Path(sys.argv[3]).resolve(), sys.argv[4] if len(sys.argv) == 5 else "")
