#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The small funnel's pictures (mitcad#1624), for tools/ui-small-funnel-look.sh.

Usage: ui-small-funnel-look.py <mitcad-cli> <dir>

The application must have opened the funnel and publish its listener in
MITCAD_MCP_DISCOVERY_DIR. Through `mitcad-cli mcp --connect` (the bridge of
tools/ui-mcp-live.py), each picture's camera is set (ui_camera) and the
view is saved (ui_screenshot) as <dir>/funnel-<name>.png.
Only the standard library is used.
"""

import base64
import importlib.util
import math
import os
import sys
from pathlib import Path

# The funnel of tools/ui-scripts/small-funnel.json: a tube of 10 mm up to
# 15 mm, a cone of 30 degrees to 25 mm, its rim (outer radius 10.77 mm)
# rounded 0.25 mm; the outlet's circle (the tube's inside, 4 mm) meets the
# cone's inside at 15.27 mm.
RIM = 10.77 - 0.25
PICTURES = [
    # name, eye's tilt from straight down and its direction round the funnel
    # (degrees), the point looked at, its distance and the field of view.
    ("whole", 20, -60, (0, 0, 14), 70, 45),
    # The rim's outer rounding where it is the outline, close up.
    ("rim", 20, -60, (RIM * math.cos(math.radians(30)), RIM * math.sin(math.radians(30)), 24.8), 40, 6),
    # The outlet's near edge from 26 degrees, where the open tube shows the
    # background through it (the reporter's "dark sliver").
    ("outlet", 26, 10, (4, 0, 15.27), 40, 12),
]


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    cli, out = sys.argv[1], Path(sys.argv[2])
    spec = importlib.util.spec_from_file_location("live", Path(__file__).with_name("ui-mcp-live.py"))
    live = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(live)
    live.wait_for_listener(cli, dict(os.environ))
    bridge = live.Bridge(cli, dict(os.environ), out / "bridge.stderr.log")
    try:
        bridge.initialize()
        document = bridge.tool("document_list", {})["documents"][0]["document"]
        for command in ("view.style.visible_edges", "view.environment.dark"):
            bridge.tool("ui_command", {"document": document, "id": command})
        for name, tilt, around, target, distance, fov in PICTURES:
            tilt, around = math.radians(tilt), math.radians(around)
            eye = [target[0] + distance * math.sin(tilt) * math.cos(around),
                   target[1] + distance * math.sin(tilt) * math.sin(around),
                   target[2] + distance * math.cos(tilt)]
            bridge.tool("ui_camera", {"eye": eye, "target": list(target), "up": [0, 0, 1],
                                      "projection": "perspective", "fov": fov})
            shot = bridge.tool("ui_screenshot", {"max_size": 1000})
            path = out / ("funnel-%s.png" % name)
            path.write_bytes(base64.b64decode(shot["image"]["data"]))
            print("ok   " + str(path))
        bridge.finish()
        return 0
    except Exception as error:  # noqa: BLE001 - the look reports any failure
        print("FAIL: " + str(error), file=sys.stderr)
        return 1
    finally:
        bridge.stop()


if __name__ == "__main__":
    sys.exit(main())
