#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/web app/files app/render/Renderer.hpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/gltf core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: app/browser/AppearanceDialog.cpp app/framework/Appearances.cpp
# check-all sources: tools/ui-scripts/gltf.mitcad-ui tools/ui-scripts/gltf-open.mitcad-ui
# check-all sources: tools/ui-scripts/gltf.json tools/ui-scripts/gltf-read.json
# glTF export and import (mitcad#1826) through the real UI, the shared input
# scripts tools/ui-scripts/gltf.mitcad-ui and gltf-open.mitcad-ui (also run
# by tools/ui-windows-gltf-test.ps1 and tools/ui-macos-gltf-test.sh), on a
# design of two components (tools/ui-scripts/gltf.json: Bar, its body red;
# Tile, turned a quarter and moved, its body textured):
#   1. File > Export to robot.glb and robot.gltf (its buffer and texture
#      image beside it); File > Import of the .gltf into the design, asked
#      where it goes; File > Import of the .glb into a new design;
#   2. MitCad's own reader (the import; no validator) reads both files
#      back with mitcad-cli (tools/ui-scripts/gltf-read.json): the same node
#      names, placements within 1e-6 m, 12 triangles a box, the colours;
#      the .glb imported as the components with their mesh bodies;
#   3. --open of the .glb: a new document of its two components; the
#      Appearances window shows the textured appearance laid by the
#      mesh's texture coordinates (Projection Texture coordinates), and
#      Box and Texture coordinates picked there edit it (mitcad#1874).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-gltf-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-gltf.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

# wait_driver what: waits for the driver's run of the application to end.
wait_driver() {
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
}

"$cli" run "$here/ui-scripts/gltf.json" --save "$work/robot.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display

MITCAD_TEST_INPUT=$here/ui-scripts/gltf.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/robot.mitcad"
wait_driver 'export and import'
ui_expect_log 'TestDriver: done' 'File > Export writes .glb and .gltf; File > Import reads both'
[ "$(head -c 4 "$work/robot.glb")" = glTF ] || ui_fail "robot.glb is no binary glTF file"
grep -q '"uri": *"robot.bin"' "$work/robot.gltf" || ui_fail "robot.gltf does not refer to robot.bin"
echo "ok   robot.glb binary, robot.gltf refers to its buffer beside it"

(cd "$work" && "$cli" run "$here/ui-scripts/gltf-read.json") > "$work/read.log" 2>&1 ||
  { cat "$work/read.log"; ui_fail "mitcad-cli: the files read back"; exit 1; }
echo "ok   both files read back: names, placements, triangles, colours; the .glb imported as components"

MITCAD_TEST_INPUT=$here/ui-scripts/gltf-open.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/robot.glb"
wait_driver 'open'
ui_expect_log 'TestDriver: done' 'File > Open of the .glb: a new document of its two components; Projection Texture coordinates in Appearances'
ui_finish 'UI glTF export and import'
