#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/commands app/MainWindowSketch3d.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/ModelShapes.cpp app/OcctViewer.cpp tools/cli
# check-all sources: core/model-api/src/sketch3d.rs core/model/src/document/browser.rs
# check-all sources: core/sketch/src/spatial.rs core/sketch/src/spatial_convert.rs core/sketch/src/spatial_edit.rs
# check-all sources: core/model/src/document/spatial_sketches.rs core/model/src/document/sketch_conversion.rs
# check-all sources: core/model/src/features/rib.rs
# check-all sources: tools/ui-scripts/sketch3d.json tools/ui-scripts/sketch3d.mitcad-ui
# 3D sketches in the Qt frontend on macOS (mitcad#1524-#1527, #1544,
# #1548-#1550): the main steps of tools/ui-sketch3d-test.sh (Linux) and
# tools/ui-windows-sketch3d-test.ps1 (Windows) as the test driver's script
# tools/ui-scripts/sketch3d.mitcad-ui on the design mitcad-cli makes
# (tools/ui-scripts/sketch3d.json): Create Sketch 2D and 3D, an existing 3D
# sketch edited, the drawing tools (Line by typed offsets and by Axis and
# Length, Fit Point Spline, Tangent Arc with typed Radius and Arc Length,
# Center Point Arc, Point, a line end On Face), editing (a dimension typed,
# driven and removed, a drag, a Length added, Fix, Delete with Dependents),
# a planar sketch converted to 3D and back, a fillet, the plane-bound
# Rectangle, Circle, Ellipse, Polygon and Text, and Rib taking a 3D
# sketch's curves in its sketch plane only.
# Usage: bash tools/ui-macos-sketch3d-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-sketch3d}"
cli=$2
UI_TIMEOUT=840
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/route.mitcad
"$cli" run "$here/ui-scripts/sketch3d.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d.mitcad-ui ui_run_app sketch3d \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log '^TestDriver: done' \
  'Create Sketch 2D and 3D, the drawing tools, editing, conversion, a fillet, the plane-bound tools and Rib'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if grep -q 'Recompute failed' "$UI_LOG_FILE"; then
  ui_fail 'a recompute failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: 3D sketches on macOS; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: 3D sketches on macOS"
