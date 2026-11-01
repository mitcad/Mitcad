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
    # Compiler process-tree memory is not bounded on macOS; timed Metal
    # scene acceptance also needs the whole machine free of competing work.
    for name in ("core.cargo_build", "core.cargo_build_release", "app.render_metal_scenes",
                 "app.render_surface_metal_macos", "app.render_surface_metal_gl_macos"):
        if name in properties:
            assert properties[name].get("RUN_SERIAL", False), name
    print(f"macOS resources: {len(desktop)} desktop tests locked; offscreen work independent")


if __name__ == "__main__":
    main()
