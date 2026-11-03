#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Check the configured macOS tests' desktop exclusion and parallel work."""
import argparse
import json
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ctest", required=True)
    parser.add_argument("--build", required=True)
    args = parser.parse_args()
    tests = json.loads(subprocess.check_output(
        [args.ctest, "--test-dir", args.build, "--show-only=json-v1"],
        text=True))["tests"]
    properties = {test["name"]: {p["name"]: p["value"]
                                 for p in test.get("properties", [])}
                  for test in tests}
    desktop = {name for name in properties if name.startswith("app.ui-macos")}
    desktop.update(name for name in (
        "repo.macos-ui-lock", "app.render_surface_macos") if name in properties)
    assert desktop, "no desktop tests configured"
    for name in sorted(desktop):
        props = properties[name]
        harness = "macos_gui_harness" in props.get("LABELS", [])
        if harness:
            # Only the audited, process-private workflows may own Aqua
            # inside ui_run_app rather than throughout their CLI setup.
            workflow = name.removeprefix("app.ui-macos-").removeprefix("native-")
            assert workflow in {"bindings-parity", "binding-card-parity", "configuration-parity",
                                "configuration-tables-parity", "slicer-parity"}, name
            assert "macos_desktop" not in props.get("RESOURCE_LOCK", []), name
        else:
            assert "macos_desktop" in props.get("RESOURCE_LOCK", []), name
        assert not props.get("RUN_SERIAL", False), name
    for name in ("app.unit", "app.render_devices"):
        props = properties[name]
        assert "QT_QPA_PLATFORM=offscreen" in props.get("ENVIRONMENT", []), name
        assert not props.get("RUN_SERIAL", False), name
        assert "macos_desktop" not in props.get("RESOURCE_LOCK", []), name
    for name in ("app.render_surface_xpc_macos", "app.render_service_macos"):
        if name in properties:
            props = properties[name]
            assert not props.get("RUN_SERIAL", False), name
            assert "macos_desktop" not in props.get("RESOURCE_LOCK", []), name
    for name in ("core.cargo_build", "core.cargo_build_release"):
        if name in properties:
            props = properties[name]
            assert not props.get("RUN_SERIAL", False), name
            assert "cargo_build" in props.get("RESOURCE_LOCK", []), name
            assert "macos_cargo_build" in props.get("LABELS", []), name
            assert "MITCAD_CARGO_MAX_JOBS=2" in props.get("ENVIRONMENT", []), name
    # Timed Metal scene acceptance needs the whole machine free of
    # competing work. Its performance assertions are unchanged.
    for name in ("app.render_metal_scenes",
                 "app.render_surface_metal_macos", "app.render_surface_metal_gl_macos"):
        if name in properties:
            assert properties[name].get("RUN_SERIAL", False), name
    harnesses = sum("macos_gui_harness" in properties[n].get("LABELS", []) for n in desktop)
    print(f"macOS resources: {len(desktop) - harnesses} CTest desktop locks; "
          f"{harnesses} harnesses lock every app launch; offscreen work independent")


if __name__ == "__main__":
    main()
