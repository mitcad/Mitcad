#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/sketch/DrawTools.cpp app/framework/TestDriver.cpp geometry/src/dressup.cpp geometry/src/guard.cpp
# check-all sources: third_party/vcpkg-ports/opencascade/portfile.cmake tools/ui-scripts/fillet-boss-tangent.mitcad-ui
# check-all sources: tools/ui-scripts/fillet-boss-tangent-crash.mitcad-ui tools/ui-scripts/fillet-boss-tangent.json tools/cli
# The part of GitHub mitcad/Mitcad#173 (mitcad#1422) built through the real
# UI and its fillet, the shared input scripts (also run by
# tools/ui-windows-fillet-boss-tangent-test.ps1):
#   1. tools/ui-scripts/fillet-boss-tangent.mitcad-ui on an empty sketch
#      that mitcad-cli makes (tools/ui-scripts/fillet-boss-tangent.json): a
#      Center to Center Slot extruded 15, a circle on its end extruded 30
#      and joined, then Fillet on the plate's top edge running into the boss
#      with Tangent Chain: 30 mm fails with its error in the panel, 3 mm
#      makes a valid rounding of the exact volume; saved.
#   2. tools/ui-scripts/fillet-boss-tangent-crash.mitcad-ui on that design,
#      with MITCAD_TEST_OCCT_CRASH=fillet: the fillet that crashes inside
#      OCCT, as the design opens and in a new Fillet's preview, is an error
#      (in the timeline, in the panel) and the application goes on.
#
# Runs headless on Xvfb (see ui-test-lib.sh), without gdb, which would stop
# at the crash that OCCT's handlers turn into an error.
# Usage: tools/ui-fillet-boss-tangent-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-fillet-boss-tangent.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
design=$work/fillet-boss-tangent.mitcad
"$cli" run "$here/ui-scripts/fillet-boss-tangent.json" --save "$design" || exit 1
ui_start_display

# run_driver what: waits for the driver's script to end the application.
run_driver() {
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed or the application ended"; exit 1; }
  UI_RUNNER=""
}

echo "--- 1. The part through the UI, and its fillet"
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-boss-tangent.mitcad-ui
UI_GDB=0 ui_start_app --ui=qt --no-mcp --open "$design"
run_driver 'the part and its fillet'
ui_expect_log 'Added slot, width 50 mm' 'the slot drawn'
ui_expect_log 'Added circle, diameter 50 mm at (85, 0)' "the circle on the slot's end"
ui_expect_log 'Added extrude (Join), distance 30' 'the boss joined'
ui_expect_log 'Panel Fillet error: ' 'the 30 mm fillet failed with its error in the panel'
ui_expect_log 'Added Fillet' 'the 3 mm fillet made'
ui_expect_log 'TestDriver: done' 'the part built through the UI and its edge rounded at 3 mm with Tangent Chain'
grep -q '"type": "fillet"' "$design" || ui_fail 'the saved design has no fillet'
echo "ok   the saved design has the fillet"
# The fillet as saved, recomputed without the UI: valid (not an error).
"$cli" info "$design" > "$work/info.log" 2>&1 || { cat "$work/info.log"; ui_fail 'mitcad-cli info'; }
grep -q 'Fillet1 (fillet): ok' "$work/info.log" || { cat "$work/info.log"; ui_fail 'Fillet1 is not ok'; }
echo "ok   Fillet1 recomputes ok"

echo "--- 2. A fillet that crashes inside OCCT"
: > "$UI_LOG"
export MITCAD_TEST_INPUT=$here/ui-scripts/fillet-boss-tangent-crash.mitcad-ui
UI_GDB=0 MITCAD_TEST_OCCT_CRASH=fillet ui_start_app --ui=qt --no-mcp --open "$design"
run_driver 'the crashing fillet'
ui_expect_log 'Preview Fillet: failed: ' "the crashing fillet's preview failed"
ui_expect_log 'Panel Fillet error: ' 'its error in the panel'
ui_expect_log 'Command Fillet cancelled' 'Escape cancelled the command, the application still running'
ui_expect_log 'TestDriver: done' 'a fillet that crashes inside OCCT is an error, not the end of the application'
ui_finish 'UI fillet of a plate edge running tangentially into a boss'
