#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dPalette.hpp app/sketch/Sketch3dModel.cpp
# check-all sources: app/sketch/Sketch3dModel.hpp app/sketch/Sketch3dEdit.cpp app/sketch/Sketch3dEdit.hpp
# check-all sources: app/sketch/Sketch3dController.cpp app/sketch/Sketch3dController.hpp app/MainWindowSketch3d.cpp
# check-all sources: app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/sketch3d-palette-words.mitcad-ui tools/ui-scripts/sketch3d-palette-words.json
# check-all sources: tools/cli
# The 3D Sketch Palette's rows in words (mitcad#1652) through the real UI,
# the shared input script tools/ui-scripts/sketch3d-palette-words.mitcad-ui
# (also run by tools/ui-windows-sketch3d-palette-words-test.ps1) on a design
# that mitcad-cli makes (tools/ui-scripts/sketch3d-palette-words.json): the
# reference curve's Linked references row reads "Reference curve 1 (an
# edge of Box1); included in 3D", no shown text has an id, a topological
# name or a coordinate, the pointer over the row paints the reference curve
# and the edge it follows, and leaving the row clears them.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-palette-words-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch3d-palette-words.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch3d-palette-words.mitcad-ui
"$cli" run "$here/ui-scripts/sketch3d-palette-words.json" --save "$work/sketch3d-palette-words.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/sketch3d-palette-words.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail '3D sketch palette words driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '| shown: Reference curve 1 (an edge of Box1); included in 3D' \
  'the reference curve'"'"'s row reads in words, its source by its feature'"'"'s name'
ui_expect_log '3D sketch palette hover: c' 'the pointer over the row was logged'
ui_expect_log 'Notice hover highlighted: 3D Sketch Palette: sketch_curve ' \
  'the row'"'"'s reference curve and its edge were painted'
ui_expect_log '3D sketch palette hover: none' 'leaving the row ended it'
ui_expect_log 'TestDriver: done' 'the rows are words and paint what they name'
# What a person reads has no id, topological name or coordinate (the
# agents' part before "| shown:" keeps them).
if grep -E '[|] shown: .*([pck][0-9]+([^0-9]|$)|[EV][{]|F[0-9]+[.:]|[0-9][.][0-9]+, -?[0-9])' "$UI_LOG"; then
  ui_fail 'a shown text has an id, a topological name or a coordinate'
fi
echo "ok   no shown text has an id, a topological name or a coordinate"
ui_finish 'UI 3D Sketch Palette words'
