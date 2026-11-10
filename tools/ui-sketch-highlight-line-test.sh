#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# A selected sketch curve's highlight keeps the curve's line type
# (mitcad#1619), through the real UI, on a design mitcad-cli makes: Sketch1
# (F1) on XY with a line c1 from (-40, 20) to (40, 20) and two points that
# widen the fitted view.
#   1. The line selected: its highlight solid, in the view (OCCT's
#      selection highlight, "Sketch highlight lines:") and in the overlay's
#      4 px highlight ("Sketch overlay lines:").
#   2. Construction (X) with the line still selected: both highlights
#      dashed at once; X again: solid.
#   3. Centerline with the line selected: both dash-dot; again: solid.
#   4. The hover alike: the line construction geometry, nothing selected,
#      the pointer over it: the overlay's hover highlight dashed.
#
# Runs headless on Xvfb (see ui-test-lib.sh). With a file name, saves a
# screenshot of the selected construction line (step 2).
# Usage: tools/ui-sketch-highlight-line-test.sh [shot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-highlight-line.XXXXXX)
FILE=$WORK/line.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [-40, 20], "end": [40, 20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-60, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 50]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync

echo "--- The line selected: a solid highlight"
ui_mark
ui_step "select the line"                ui_sketch_click 0 20
ui_expect_new "Selected: 1 sketch curve" "the line selected"
ui_expect_new "Sketch highlight lines: 1 solid" "the view's highlight solid"
ui_expect_new "Sketch overlay lines: 1 solid" "the overlay's highlight solid"

echo "--- Construction (X) on the selected line: its highlight dashed"
ui_mark
ui_step "construction (X)"               ui_key x
ui_expect_new "Construction on:" "the line is construction geometry"
ui_expect_new "Sketch highlight lines: 1 dashed" "the view's highlight dashed at once"
ui_expect_new "Sketch overlay lines: 1 dashed" "the overlay's highlight dashed at once"
ui_sync
if [ -n "$SHOT" ]; then
  ui_capture "$SHOT"
  echo "note: screenshot $SHOT"
fi
ui_mark
ui_step "construction again (X)"         ui_key x
ui_expect_new "Construction off:" "the line is normal geometry again"
ui_expect_new "Sketch highlight lines: 1 solid" "the view's highlight solid again"
ui_expect_new "Sketch overlay lines: 1 solid" "the overlay's highlight solid again"

echo "--- Centerline on the selected line: its highlight dash-dot"
ui_mark
ui_step "centerline (search)"            ui_command "Centerline"
ui_expect_new "Centerline on:" "the line is a centre line"
ui_expect_new "Sketch highlight lines: 1 dash-dot" "the view's highlight dash-dot"
ui_expect_new "Sketch overlay lines: 1 dash-dot" "the overlay's highlight dash-dot"
ui_mark
ui_step "centerline again (search)"      ui_command "Centerline"
ui_expect_new "Centerline off:" "the line is no centre line"
ui_expect_new "Sketch highlight lines: 1 solid" "the view's highlight solid again"
ui_expect_new "Sketch overlay lines: 1 solid" "the overlay's highlight solid again"

echo "--- The hover of a construction line: dashed"
ui_mark
ui_step "construction (X)"               ui_key x
ui_expect_new "Construction on:" "the line is construction geometry"
ui_mark
# Off the x axis, which a click selects (mitcad#899).
ui_step "click where nothing is"         ui_sketch_click 30 -30
ui_expect_new "Selected: nothing" "the selection cleared"
ui_expect_new "Sketch highlight lines: none" "no curve highlighted in the view"
ui_expect_new "Sketch overlay lines: none" "no curve highlighted in the overlay"
ui_mark
ui_step "point at the line"              xdotool mousemove $(ui_sketch_at -10 20)
ui_expect_new "Sketch overlay lines: 1 dashed" "the hover's highlight dashed, as the line"

ui_finish "UI sketch highlight line test"
