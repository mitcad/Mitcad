#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowDecals.cpp app/view/DecalView.cpp app/view/Decals.cpp app/browser app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/decals.rs core/model-api/src/decals.rs tools/ui-scripts/decal.mitcad-ui tools/ui-scripts/decal.json
# Decals (mitcad#2244): the shared input script tools/ui-scripts/decal.mitcad-ui of
# tools/ui-decal-test.sh in the Qt frontend. A decal put on a box's top and
# on a rod's side, turned and flipped, checked in the log and by the colour
# of the view's pixels on the faces, and hidden behind another body.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-decal-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# A PNG file of 16 x 8 pixels: the left 12 columns opaque red, the right 4
# transparent (the same bytes as the Linux and Windows tests write); the
# system's base64 decodes with -D before macOS 13.
png='iVBORw0KGgoAAAANSUhEUgAAABAAAAAICAYAAADwdn+XAAAAF0lEQVR42mP4z8DwnxTMgA5GDRgOBgAARMC/QQD+4HYAAAAASUVORK5CYII='
printf '%s' "$png" | base64 -d > "$MITCAD_TEST_WORK/decal.png" 2> /dev/null ||
  printf '%s' "$png" | base64 -D > "$MITCAD_TEST_WORK/decal.png" || exit 1
"$cli" run "$here/ui-scripts/decal.json" --save "$MITCAD_TEST_WORK/decal.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/decal.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app decal --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/decal.mitcad"
ui_expect_log 'Decals drawn: Decal1 20.0 x 10.0 mm on a plane turned 90 deg flipped 100%' 'the decal turned and flipped'
ui_expect_log 'Decal2 15.0 x 7.5 mm on a cylinder 100%' 'the decal on the rod'
ui_expect_log '^TestDriver: done' 'Decals (mitcad#2244): every step of the script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Decals (mitcad#2244)'
