#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/commands/PrintAnalysis.cpp app/commands/PrintAnalysis.hpp
# check-all sources: app/MainWindowPrintAnalysis.cpp app/view/PrintOverlay.cpp app/view/PrintOverlay.hpp
# check-all sources: app/OcctViewer.hpp app/OcctViewer.cpp app/MainWindow.cpp app/MainWindow.hpp app/browser app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandSession.hpp app/framework/CommandPanel.cpp app/framework/Command.hpp
# check-all sources: core/model/src/document/analyses.rs core/model-api/src core/model/src/print_analysis.rs tools/cli
# Print Analysis through the real UI (mitcad#620), on a synthetic T: a
# stem of 10 x 20 x 20 mm (x = 15 to 25) under a bar of 40 x 20 x 0.5 mm
# (z = 20 to 20.5), one body (Body1). Area 2860 mm², volume 4400 mm³; the
# bar's undersides beside the stem (2 x 15 x 20 mm) overhang, the stem's
# bottom lies on the bed, and the bar is 0.5 mm thick.
#   - Inspect > Print Analysis opens its panel with the shown body: the
#     overhang 600 mm² (21.0 % of the area), the thinnest wall 0.50 mm, the
#     estimate with the defaults (PLA: 3.2 g, 1.1 m); the view draws the
#     overhang and thin-wall triangles, the marker and the legend; the thin
#     walls only beside the stem, not on the bar's top over it (mitcad#767).
#   - OK keeps it as Print1 in the browser's Analysis folder, shown; the
#     light bulb hides and shows it; a double-click edits it: the bar's top
#     as the bed face (the T upside down: no overhang) and a 0.4 mm minimum
#     wall (no thin region).
#   - Saved with the file, the bodies' STL export is the same as before the
#     analysis: the analysis changes no body.
#   - Deleted from its context menu, and undone.
#   - Orbited with Print1 shown and with its panel open (mitcad#768): the
#     view logs the navigation; nothing is computed or drawn again, the
#     overlay is not set again, and with Print1 shown no widget over the
#     view is painted again (the marker and the legend are drawn by the
#     view).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-analysis-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-print-analysis.XXXXXX)
FILE=$WORK/tee.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/tee.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [15, 0], "width": 10, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 40, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "start": {"type": "offset", "offset": 20},
    "extent": {"type": "distance", "distance": 0.5}, "operation": "join", "participants": ["F2.b0"]}}
]
EOF
"$CLI" run "$WORK/tee.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# What 3D Print gets of the bodies before any analysis.
"$CLI" export "$FILE" "$WORK/before.stl" --refinement high > "$WORK/export.log" 2>&1 ||
  { cat "$WORK/export.log"; ui_fail "the STL export before"; }

# expect_new_re "extended regex" "description": as ui_expect_new, a pattern.
expect_new_re() {
  for _ in $(seq 1 30); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line like '$1' since the mark"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the T shown"

echo "--- The panel"
ui_mark
ui_step "print analysis (search)"          ui_command "Print Analysis"
ui_expect_new "Command Print Analysis started" "its panel opens"
ui_expect_new "Print Analysis Bodies: 1 body" "with the shown body"
ui_expect_new "Inspect Print Analysis: Body1 overhang 600.0 mm^2 (21.0 %), thinnest 0.50 mm, " \
  "the bar's undersides overhang, the bar is 0.5 mm thick" 20
ui_expect_new ", estimate 3.2 g 1.1 m" "the estimate with the defaults (PLA)"
expect_new_re "^Print overlay: [1-9][0-9]* overhang triangle\(s\) #[0-9a-f]{6}, [1-9][0-9]* thin triangle\(s\) #[0-9a-f]{6} \([0-9.]+ mm\^2\), marker Thinnest 0\.50 mm at \(" \
  "the view draws the overhangs, the thin walls and the thinnest point"
# Only where the bar is thin (mitcad#767): its undersides beside the stem
# (600 mm²) and its top beside the stem (600 mm², drawn in pieces about a
# millimetre wide along the stem's sides), not the top over the stem: the
# bar's top triangles span it, and whole they made about 1400 mm².
expect_new_re "^Print overlay: .* thin triangle\(s\) #[0-9a-f]{6} \(1[12][0-9]{2}\.[0-9] mm\^2\)" \
  "the thin walls drawn beside the stem only"
ui_expect_new "legend Print Analysis: " "and the legend"
ui_expect_new "Overhang, angle 45°" "with the angle"
ui_expect_new "Thin wall, under 0.8 mm" "and the minimum wall"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Kept in the Analysis folder"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Kept Print Analysis as Print1" "kept in the document"
ui_expect_new "Browser Root/Analysis/Print1 at" "the Analysis folder lists it"
ui_expect_new "Print analysis Print1: Body1 overhang 600.0 mm^2 (21.0 %), thinnest 0.50 mm, " \
  "shown when the panel has closed" 20
ui_expect_new "legend Print1: " "its overlay with its name"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Section analysis at" && ui_fail "a print analysis cut the bodies"
echo "ok   no section"

echo "--- The light bulb"
ui_mark
ui_step "hide it (light bulb)"             ui_click_logged "Browser eye Root/Analysis/Print1"
ui_expect_new "Visibility Root/Analysis/Print1: hidden" "hidden with its light bulb"
ui_expect_new "Print overlay: none" "nothing drawn"
ui_mark
ui_step "show it (light bulb)"             ui_click_logged "Browser eye Root/Analysis/Print1"
ui_expect_new "Visibility Root/Analysis/Print1: shown" "shown with its light bulb"
ui_expect_new "legend Print1: " "drawn again"

echo "--- Edit: the T upside down, a thinner minimum wall"
ui_double_click_expect "Browser Root/Analysis/Print1" "Editing analysis Print1 with Print Analysis" "its panel opens"
ui_expect_new "Inspect Print Analysis: Body1 overhang 600.0 mm^2" "with its settings" 20
ui_mark
ui_step "the Bed face input"               ui_click_logged "Panel Print Analysis input bed_face"
ui_step "pick the bar's top"               ui_click_pick "/F4:end($R)"
ui_expect_new "Print Analysis Bed Face: 1 face" "the bar's top as the bed face"
ui_expect_new "Inspect Print Analysis: Body1 overhang 0.0 mm^2 (0.0 %)" "nothing overhangs upside down" 20
ui_mark
ui_step "minimum wall 0.4"                 ui_type_in "Panel Print Analysis input minimum" "0.4"
ui_expect_new "thinnest 0.50 mm, 0 thin region(s)" "no wall under 0.4 mm" 20
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited analysis Print1" "the analysis changed"
ui_expect_new "Print analysis Print1: Body1 overhang 0.0 mm^2 (0.0 %), thinnest 0.50 mm, 0 thin region(s), estimate 3.2 g 1.1 m" \
  "shown with its new settings" 20
ui_expect_new "Thin wall, under 0.4 mm" "the legend's minimum"

echo "--- Saved; the bodies as before"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
grep -q '"print"' "$FILE" || ui_fail "no print analysis in the file"
echo "ok   the analysis is in the file"
"$CLI" export "$FILE" "$WORK/after.stl" --refinement high > "$WORK/export.log" 2>&1 ||
  { cat "$WORK/export.log"; ui_fail "the STL export after"; }
cmp -s "$WORK/before.stl" "$WORK/after.stl" || ui_fail "the analysis changed what 3D Print gets"
echo "ok   the STL export is the same as before the analysis"

echo "--- Delete and undo"
ui_mark
ui_step "right-click Print1"               ui_click_logged "Browser Root/Analysis/Print1" 3
ui_expect_new "Context menu: Edit Print Analysis | Hide | Rename | Delete" "its menu"
ui_step "choose Delete"                    ui_menu_choose "Delete"
ui_expect_new "Deleted analysis Print1" "deleted"
ui_expect_new "Print overlay: none" "nothing drawn"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Delete Print1" "undone"
ui_expect_new "legend Print1: " "drawn again"

echo "--- Orbits compute nothing again and paint no widget over the view (mitcad#768)"
# orbit: a Shift + middle drag across the view (an orbit in the default
# navigation) in twelve steps; the view logs the navigation once it ends.
orbit() {
  local i x y
  read -r x y <<< "$(ui_view_at 25 70)"
  ui_apart "$x" "$y"
  xdotool mousemove "$x" "$y" keydown shift mousedown 2
  for i in $(seq 1 12); do
    xdotool mousemove $((x + i * 12)) $((y - i * 3))
    sleep 0.05
  done
  xdotool mouseup 2 keyup shift
}
# orbited "what" max-repaints: the navigation's line since the mark: frames
# drawn, no Print Analysis overlay set and nothing computed again, and at
# most that many repaints of widgets over the view (the house button
# may change once as the camera leaves its home; the marker and the
# legend are drawn by the view itself: a widget over the view would be
# painted again at each frame).
orbited() {
  local line frames repaints
  expect_new_re "^View navigation: [0-9]+ frame\(s\), [0-9]+ repaint\(s\) of widgets over the view" \
    "$1: the orbit ended"
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^View navigation: " | tail -1)
  frames=$(sed -E 's/^View navigation: ([0-9]+) frame.*/\1/' <<< "$line")
  repaints=$(sed -E 's/^.* frame\(s\), ([0-9]+) repaint.*/\1/' <<< "$line")
  [ "$frames" -ge 6 ] || ui_fail "$1: the orbit drew only $frames frame(s): $line"
  echo "ok   $1: the orbit drew $frames frames"
  case "$line" in
  *", Print Analysis overlay set 0 time(s)") echo "ok   $1: no overlay set during the orbit" ;;
  *) ui_fail "$1: the overlay was set during the orbit: $line" ;;
  esac
  [ "$repaints" -le "$2" ] || ui_fail "$1: widgets over the view painted again during the orbit: $line"
  echo "ok   $1: $repaints repaint(s) of widgets over the view"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    grep -E "^(Print overlay: |Inspect Print Analysis: |Print analysis Print1: |Wait cursor on: print_)" &&
    ui_fail "$1: the analysis was computed or drawn again during the orbit"
  echo "ok   $1: nothing computed again"
}
ui_mark
ui_step "orbit with Print1 shown"          orbit
orbited "Print1 shown" 2
ui_double_click_expect "Browser Root/Analysis/Print1" "Editing analysis Print1 with Print Analysis" "its panel opens"
ui_expect_new "Inspect Print Analysis: Body1 overhang 0.0 mm^2" "with its settings" 20
ui_mark
ui_step "orbit with the panel open"        orbit
# A value field of the panel may keep its parameter labels (a widget over
# the view) for a moment after the click into the view.
orbited "the panel open" 20

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI print analysis test"
