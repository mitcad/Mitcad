#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/gravity app/MainWindowGravity.cpp app/commands/GravityCommands.cpp app/framework/CommandPanel.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowContextMenu.cpp app/browser/BrowserPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp app/framework/DocumentSession.cpp
# check-all sources: core/model-api/src/gravity.rs core/model/src/gravity.rs core/model/src/document/gravity.rs
# check-all sources: core/model-api/src/flex_hang.rs core/model/src/document/flex_hang.rs core/model-base/src/flex_drape.rs
# check-all sources: core/ffi/src/kernel/flex_drape.rs core/cpp/bridge/flex_drape.cpp geometry/src/flex_drape.cpp
# check-all sources: tools/ui-scripts/gravity.mitcad-ui tools/ui-scripts/gravity.json tools/cli
# check-all sources: app/flex/FlexRope.cpp app/flex/FlexEdit.cpp core/model/src/features/flex.rs
# Let It Hang and its panel's direction of gravity (mitcad#1713) through
# the real UI, the shared input script tools/ui-scripts/gravity.mitcad-ui
# (also run by tools/ui-windows-gravity-test.ps1 and
# tools/ui-macos-gravity-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/gravity.json: a plate, a step on it, a plank above the
# step's edge and a hose over them, no direction of gravity), in the
# Docked and in the Floating layout: Let It Hang from the hose's context
# menu asks for a direction; the plate's top face picked, the hose falls
# from its ends, its middle onto the plank where it rests lying on it; its
# middle node pinned and let go; the damping changed; dragged by its middle
# and let go; OK keeps its shape with the direction as one undo step, Undo
# takes both back; from the ribbon again, the length longer and its end
# let go, Esc puts it back and takes the direction back; once more, Clear
# Gravity in the panel, a face again, Esc. The marker only while the
# panel is open; no row of gravity in the browser.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-gravity-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-gravity.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/gravity.mitcad-ui
ui_start_display

# Where the hose rested before it was kept (the last "Hang rest F4" line
# before "Hang locked F4"): lying on the plank (three of its points or
# more on a body) and nowhere through the plate (its lowest point at least
# the plate's top, 10, and the hose's radius, 3, high). Fallen through the
# plank it would touch nothing.
check_rest() {
  local line lowest touching
  line=$(grep -a -B1000 '^Hang locked F4: ' "$UI_LOG" | grep -a '^Hang rest F4: ' | tail -n 1)
  [ -n "$line" ] || { ui_fail "no resting hose logged ($1)"; exit 1; }
  lowest=$(sed -nE 's/.*, lowest ([-0-9.]+) along .*/\1/p' <<< "$line")
  touching=$(sed -nE 's/.*, touching ([0-9]+), .*/\1/p' <<< "$line")
  [ -n "$lowest" ] && [ -n "$touching" ] || { ui_fail "the rest line cannot be read ($1): $line"; exit 1; }
  if ! awk -v z="$lowest" -v t="$touching" 'BEGIN { exit !(z >= 12.9 && t >= 3) }'; then
    ui_fail "the hose does not lie on the plank ($1): $line"
    exit 1
  fi
  echo "ok   the hose lies on the plank, $touching points on it, its lowest point at $lowest ($1)"
}

for chrome in docked floating; do
  echo "--- Let It Hang in the $chrome layout"
  "$cli" run "$here/ui-scripts/gravity.json" --save "$work/gravity-$chrome.mitcad" || exit 1
  ui_start_app --ui=qt --no-mcp "--chrome=$chrome" --open "$work/gravity-$chrome.mitcad"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the gravity driver timed out ($chrome)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'Hang panel opened' "Let It Hang opened its panel from the hose's context menu ($chrome)"
  ui_expect_log 'Preview Let It Hang: not built: Pick a planar face' "without a direction the panel asks for one ($chrome)"
  ui_expect_log 'Gravity direction: 0.000,0.000,-1.000 from face' "gravity pulls against the plate's top face ($chrome)"
  ui_expect_log 'Gravity marker: arrow ' "the marker shows the direction while the panel is open ($chrome)"
  ui_expect_log 'Hang started F4 (Hose 6): ' "the hose let hang once the direction was given ($chrome)"
  ui_expect_log "Hang press beside F4: the view's" "a press beside the hose is the view's ($chrome)"
  ui_expect_log 'Hang pin n2 pinned' "a click on the middle node pinned it ($chrome)"
  ui_expect_log 'Hang pin n2 let go' "another click let it go ($chrome)"
  ui_expect_log 'Hang damping 0.800' "the damping changed in the panel ($chrome)"
  ui_expect_log 'Hang drag of F4 started at ' "a press on the hose grabbed it ($chrome)"
  ui_expect_log 'Hang drag of F4 released after ' "the release let it settle ($chrome)"
  ui_expect_log 'Hang locked F4: nodes n1,' "OK kept its shape ($chrome)"
  ui_expect_log 'Let It Hang: its steps merged into Edit Flex1' "the direction and the shape are one undo step ($chrome)"
  check_rest "$chrome"
  ui_expect_log 'Gravity marker: none' "the marker went with the panel ($chrome)"
  ui_expect_log 'Undo: Edit Flex1' "one undo step takes them back ($chrome)"
  ui_expect_log 'Hang length 200.000 mm' "the length changed in the panel ($chrome)"
  ui_expect_log 'Hang ends pinned free' "End pinned cleared let its end go ($chrome)"
  ui_expect_log 'Hang cancelled: F4 put back' "Esc put it back ($chrome)"
  ui_expect_log 'Let It Hang: 1 direction change(s) taken back' "Esc took the direction back ($chrome)"
  ui_expect_log 'Gravity cleared' "Clear Gravity in the panel cleared the direction ($chrome)"
  ui_expect_log 'Let It Hang: 3 direction change(s) taken back' "Esc took every direction change back ($chrome)"
  ui_expect_log 'TestDriver: done' "the script ran to its end ($chrome)"
  ui_stop_app
done
ui_finish 'UI Gravity'
