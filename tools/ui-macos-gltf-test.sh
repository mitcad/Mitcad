#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/web app/files app/render/Renderer.hpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/gltf core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: app/browser/AppearanceDialog.cpp app/framework/Appearances.cpp
# glTF export and import (mitcad#1826) on macOS in Qt, as
# tools/ui-gltf-test.sh on Linux: the shared input scripts
# tools/ui-scripts/gltf.mitcad-ui and gltf-open.mitcad-ui on a design of two
# components (tools/ui-scripts/gltf.json): File > Export to robot.glb and
# robot.gltf (its buffer and texture image beside it), File > Import of
# both; MitCad's own reader reads both files back with mitcad-cli
# (tools/ui-scripts/gltf-read.json: node names, placements within 1e-6 m,
# triangles, colours, the .glb imported as components); --open of the .glb
# a new document of its two components, whose textured appearance the
# Appearances window shows by its texture coordinates, Projection Box and
# Texture coordinates picked there (mitcad#1874).
# Usage: tools/ui-macos-gltf-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-gltf-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/gltf.json" --save "$MITCAD_TEST_WORK/robot.mitcad" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log" >&2; echo 'FAIL: mitcad-cli: the design' >&2; exit 1; }

MITCAD_TEST_INPUT=$here/ui-scripts/gltf.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app gltf --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$MITCAD_TEST_WORK/robot.mitcad"
ui_expect_log '^TestDriver: done' 'File > Export writes .glb and .gltf; File > Import reads both'
ui_expect_no_log 'TestDriver: FAIL' 'no failed glTF export or import step'
[ "$UI_FAILURES" = 0 ] || exit 1
[ "$(head -c 4 "$MITCAD_TEST_WORK/robot.glb")" = glTF ] || { echo 'FAIL: robot.glb is no binary glTF file' >&2; exit 1; }
grep -q '"uri": *"robot.bin"' "$MITCAD_TEST_WORK/robot.gltf" ||
  { echo 'FAIL: robot.gltf does not refer to robot.bin' >&2; exit 1; }
echo 'ok   robot.glb binary, robot.gltf refers to its buffer beside it'

(cd "$MITCAD_TEST_WORK" && "$cli" run "$here/ui-scripts/gltf-read.json") > "$UI_OUT/read.log" 2>&1 ||
  { cat "$UI_OUT/read.log" >&2; echo 'FAIL: mitcad-cli: the files read back' >&2; exit 1; }
echo 'ok   both files read back: names, placements, triangles, colours; the .glb imported as components'

MITCAD_TEST_INPUT=$here/ui-scripts/gltf-open.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app gltf-open --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$MITCAD_TEST_WORK/robot.glb"
ui_expect_log '^TestDriver: done' 'File > Open of the .glb: a new document of its two components'
ui_expect_no_log 'TestDriver: FAIL' 'no failed glTF open step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt glTF export and import'
