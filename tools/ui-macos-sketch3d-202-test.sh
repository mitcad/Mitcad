#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/commands app/MainWindowSketch3d.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp app/view/ViewController.cpp tools/cli
# check-all sources: core/model-api/src/sketch3d.rs core/sketch/src/spatial_convert.rs
# check-all sources: core/model/src/features/spatial_path.rs core/model/src/features/pipe.rs
# check-all sources: tools/ui-scripts/sketch3d-202.json tools/ui-scripts/sketch3d-202.mitcad-ui
# The scenario of GitHub mitcad/Mitcad#202 end to end (mitcad#1550, item 7)
# in the Qt frontend on macOS: the steps tools/ui-scripts/sketch3d-202.mitcad-ui
# (a rectangle sketched on Box1's top face, the Sketch Palette's 3D Sketch
# box, a line off the face by typed offsets and a tangent arc onto a top
# corner of the component Post's box, a Pipe along them with Chain
# Selection off, saved) on the design mitcad-cli makes
# (tools/ui-scripts/sketch3d-202.json); then, from the run's log and the
# saved design: the pipe's volume is an 8 mm pipe of the route's length,
# Sketch1 is still a 3D sketch, the pipe's path is its two curves, its start
# face at the rectangle's corner and its end face at Post's vertex (the
# measure query). As tools/ui-sketch3d-202-test.sh (Linux) and
# tools/ui-windows-sketch3d-202-test.ps1 (Windows).
# Usage: bash tools/ui-macos-sketch3d-202-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-sketch3d-202}"
cli=$2
UI_TIMEOUT=420
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/route.mitcad
"$cli" run "$here/ui-scripts/sketch3d-202.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d-202.mitcad-ui ui_run_app sketch3d-202 \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log '^TestDriver: done' 'the rectangle on the face, 3D Sketch checked, the route onto Post, the pipe, saved'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: 3D sketch of GitHub mitcad/Mitcad#202; evidence in $UI_OUT"
  exit 1
fi

# What the run logged: the frame's origin (its X and normal are the
# design's, the script checked), the corner A, the vertex V (local
# coordinates), the line and the arc, the pipe's volume.
last() { sed -n "s/$1/\\1/p" "$UI_LOG_FILE" | tail -n 1; }
origin=$(last '^3D sketch frame in the design: origin (\([^)]*\)).*')
corner=$(last '^3D sketch Line: pending point 1 at (\([^)]*\)).*')
vertex=$(last '^3D sketch snap: Linked Point: vertex .* at (\([^)]*\))$')
line=$(last '^3D sketch Line: made \(c[0-9]*\) from .*')
arc=$(last '^3D sketch Tangent Arc: made \(c[0-9]*\) from .*')
volume=$(last '^New body [^ ]* (F4\.b0): volume \([0-9.]*\) mm3.*')
for value in "$origin" "$corner" "$vertex" "$line" "$arc" "$volume"; do
  [ -n "$value" ] || { ui_fail "the log lacks the frame, the corner, the vertex, the curves or the pipe's volume"; exit 1; }
done
# shellcheck disable=SC2086
set -- $(echo "$origin $corner $vertex" | tr -d ',')
ox=$1 oy=$2 oz=$3 ax=$4 ay=$5 az=$6 vx=$7 vy=$8 vz=$9
# Where A and V are in the design; V a top corner of Post's box; the route's
# length: AB (10, 0, 15), then the arc from B along AB's direction to V, the
# chord BV at an angle phi to it: 2 phi round, |BV| phi / sin phi long.
read -r dax day daz dvx dvy dvz expected <<< "$(awk -v ox="$ox" -v oy="$oy" -v oz="$oz" \
  -v ax="$ax" -v ay="$ay" -v az="$az" -v vx="$vx" -v vy="$vy" -v vz="$vz" 'BEGIN {
  pi = atan2(0, -1); ab = sqrt(10 * 10 + 15 * 15)
  cx = vx - (ax + 10); cy = vy - ay; cz = vz - (az + 15); c = sqrt(cx * cx + cy * cy + cz * cz)
  cosphi = (10 * cx + 15 * cz) / (ab * c); phi = atan2(sqrt(1 - cosphi * cosphi), cosphi)
  printf "%.6f %.6f %.6f %.6f %.6f %.6f %.6f\n", ox + ax, oy + ay, oz + az, ox + vx, oy + vy, oz + vz,
    pi * 16 * (ab + c * phi / sin(phi)) }')"
if awk -v x="$dvx" -v y="$dvy" -v z="$dvz" 'BEGIN {
  ok = ((x - 80) ^ 2 < 1e-6 || (x - 100) ^ 2 < 1e-6) && ((y - 60) ^ 2 < 1e-6 || (y - 80) ^ 2 < 1e-6) && (z - 50) ^ 2 < 1e-6
  exit !ok }'; then
  ui_pass "V ($dvx, $dvy, $dvz) is a top corner of Post's box"
else
  ui_fail "V ($dvx, $dvy, $dvz) is not a top corner of Post's box"
fi
if awk -v v="$volume" -v e="$expected" 'BEGIN { exit !((v - e) ^ 2 < (1e-3 * e) ^ 2) }'; then
  ui_pass "an 8 mm pipe along the whole route: $volume mm3"
else
  ui_fail "the pipe's volume $volume mm3, expected $expected"
fi
cat > "$MITCAD_TEST_WORK/check.json" << EOF
[
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {"name": "Sketch1", "type": "spatial_sketch", "status": "ok"}}},
  {"expect": {"query": {"query": "feature", "uid": "F4"}, "result": {"name": "Pipe1", "type": "pipe", "status": "ok",
    "def": {"path": {"spatial_sketch": "F3", "curves": ["$line", "$arc"]}}}}},
  {"expect": {"query": {"query": "measure", "a": {"body": "F4.b0", "face": "F4:start"}},
    "result": {"a": {"plane": {"origin": [$dax, $day, $daz]}}}, "tolerance": 1e-4}},
  {"expect": {"query": {"query": "measure", "a": {"body": "F4.b0", "face": "F4:end"}},
    "result": {"a": {"plane": {"origin": [$dvx, $dvy, $dvz]}}}, "tolerance": 1e-4}}
]
EOF
if "$cli" run "$MITCAD_TEST_WORK/check.json" --open "$design" > "$UI_OUT/check.log" 2>&1; then
  ui_pass "Sketch1 stayed a 3D sketch; the pipe's path is $line and $arc of it, from A to Post's vertex V"
else
  sed 's/^/     | /' "$UI_OUT/check.log"
  ui_fail "the saved design: not a 3D sketch, or the pipe not from the corner to Post's vertex"
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: 3D sketch of GitHub mitcad/Mitcad#202; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: 3D sketch of GitHub mitcad/Mitcad#202 on macOS"
