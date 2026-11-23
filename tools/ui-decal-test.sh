#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowDecals.cpp app/view/DecalView.cpp app/view/Decals.cpp app/browser app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/decals.rs core/model-api/src/decals.rs tools/ui-scripts/decal.mitcad-ui tools/ui-scripts/decal.json
# Decals (mitcad#2244) through the real UI: the shared input script
# tools/ui-scripts/decal.mitcad-ui (also run by
# tools/ui-windows-decal-test.ps1 and tools/ui-macos-decal-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/decal.json) and a PNG file
# written here. A decal put on a box's top and on a rod's side, turned and
# flipped, checked in the log and by the colour of the view's pixels on
# the faces, hidden behind another body, and one cut at the top's edge
# (mitcad#2266).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-decal-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-decal.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/decal.mitcad-ui
# A PNG file of 16 x 8 pixels: the left 12 columns opaque red, the right 4
# transparent (the same bytes as the Windows and macOS tests write).
png='iVBORw0KGgoAAAANSUhEUgAAABAAAAAICAYAAADwdn+XAAAAF0lEQVR42mP4z8DwnxTMgA5GDRgOBgAARMC/QQD+4HYAAAAASUVORK5CYII='
printf '%s' "$png" | base64 -d > "$work/decal.png" || exit 1
[ "$(wc -c < "$work/decal.png")" = 80 ] || { echo 'the PNG file is not 80 bytes' >&2; exit 1; }
design=$work/decal.mitcad
"$cli" run "$here/ui-scripts/decal.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'decal driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Decals drawn: Decal1 20.0 x 10.0 mm on a plane turned 90 deg flipped 100%' 'the decal turned and flipped'
ui_expect_log 'Decal2 15.0 x 7.5 mm on a cylinder 100%' 'the decal on the rod'
ui_expect_log 'Decal2 20.0 x 10.0 mm on a plane 100% cut' 'the decal cut at the edge of the top (mitcad#2266)'
ui_expect_log 'TestDriver: done' 'Decals (mitcad#2244): every step of the script'
ui_finish 'UI decal'
