#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/browser app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/helix_curve.rs core/model/src/features/intersection_curve.rs
# check-all sources: core/model/src/features/helix.rs geometry/src/intersection_curve.cpp
# check-all sources: geometry/src/surface_sweeps.cpp tools/cli core/model
# The curve tools (mitcad#1777) through the real UI, each made through its
# panel:
#   - Helix Curve on Cylinder1's side face (20 mm diameter, 20 high about
#     Z), Radius from Face, 4 turns of pitch 5: previewed from (10, 0, 0) up
#     to (10, 0, 20), drawn and listed under Construction; a 2 mm Pipe
#     along it, picked on the timeline: pi 1^2 times the helix's length
#     4 sqrt((20 pi)^2 + 5^2);
#   - Surface Helix of Sketch1's line from the axis x = 60 out to x = 75,
#     about Sketch1's vertical line, 4 turns of pitch 5: a helicoid, a
#     surface body;
#   - Intersection Curve of the helicoid's body and Cylinder2's (20 mm
#     diameter about the same axis, 50 high symmetric about XY), picked on
#     their Browser rows: one helix of radius 10, previewed from (70, 0, 0)
#     up to (70, 0, 20); a 2 mm Pipe along it of the first pipe's volume;
#   - the design file keeps the three: the helix curve without a radius of
#     its own, the surface helix of the line, the intersection of the two
#     bodies.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-curve-tools-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-curve-tools.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# The pipes' volume: pi 1^2 * 4 sqrt((20 pi)^2 + 5^2).
PIPE=792.0644067107494
# expect_file file 'python expression of doc' expected what
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}
# expect_curve what from to: the last preview curve logged since the mark
# runs from `from` to `to` (each "x.xxx, y.yyy, z.zzz").
expect_curve() {
  ui_expect_new "Preview curve: " "$1: a preview curve logged"
  local curve
  curve=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Preview curve: " | tail -1 | sed 's/.*Preview curve: //')
  echo "$curve" | grep -qF "from ($2) to ($3)" || ui_fail "$1: the preview curve is '$curve'"
  echo "ok   $1: $curve"
}

# Cylinder1 (F1, Body1): 20 mm diameter, 20 high on XY about the Z axis.
# Sketch1 (F2) on XZ: the line c1 from the axis x = 60 out to x = 75, and
# the axis line c4 from (60, -1) to (60, -11) (the sketch's y is -Z: up
# along +Z), apart from c1 so that Chain Selection does not join them.
# Cylinder2 (F3, Body2): 20 mm diameter about x = 60, 50 high symmetric
# about XY.
cat > "$WORK/curves.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": {"type": "origin", "plane": "xy"},
    "center": [0, 0], "diameter": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [60, 0], "end": [75, 0]},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [60, -1], "end": [60, -11]},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": {"type": "origin", "plane": "xy"},
    "center": [60, 0], "diameter": 20, "height": 50, "symmetric": true, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/curves.json" --save "$WORK/curves.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$WORK/curves.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Helix Curve on Cylinder1's side face"
ui_mark
ui_step "helix curve (search)"             ui_command "Helix Curve"
ui_expect_new "Command Helix Curve started" "the command started"
ui_step "the axis input"                   ui_click_logged "Panel Helix Curve input axis"
ui_step "pick the side face"               ui_click_pick "face F1.b0/F1:side0"
ui_step "pitch 5"                          ui_type_in "Panel Helix Curve input pitch" "5"
ui_step "revolutions 4"                    ui_type_in "Panel Helix Curve input revolutions" "4"
ui_expect_new "Preview Helix Curve: ok" "previewed"
expect_curve "the helix on the face" "10.000, 0.000, 0.000" "10.000, 0.000, 20.000"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added helix curve, 4 turns of 5, radius from the face, right-handed" "the helix curve added"
ui_expect_new "Construction shown: HelixCurve1" "the curve drawn in the view"
ui_step "the curve in the Browser"         ui_browser_reveal "Browser Root/Construction/HelixCurve1"
ui_place_valid "Browser Root/Construction/HelixCurve1" || ui_fail "HelixCurve1 is not under Construction"
echo "ok   listed under Construction"

echo "--- Pipe along it, picked on the timeline"
# The size before the path: Pipe's default 8 mm section is wider than the
# 5 mm pitch, so its turns pass through each other and the first preview
# would build the folded pipe in pieces (about two minutes) before the
# 2 mm one.
ui_mark
ui_step "pipe (search)"                    ui_command "Pipe"
ui_step "section size 2"                   ui_type_in "Panel Pipe input size" "2"
ui_step "the path input"                   ui_click_logged "Panel Pipe input path"
ui_step "pick HelixCurve1"                 ui_click_logged "Timeline HelixCurve1"
ui_expect_new "Pipe Path: 1 feature [feature F4]" "the helix curve picked as the path"
ui_expect_new "Preview Pipe: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body .* (F5.b0): volume \([0-9.]*\) mm3" "$PIPE" "a 2 mm pipe along the helix" 1e-3

echo "--- Surface Helix of the line: a helicoid"
ui_mark
ui_step "surface helix (search)"           ui_command "Surface Helix"
ui_expect_new "Command Surface Helix started" "the command started"
ui_step "the profiles input"               ui_click_logged "Panel Surface Helix input profiles"
ui_step "pick the line"                    ui_click_logged "Sketch entity F2/c1"
ui_step "the axis input"                   ui_click_logged "Panel Surface Helix input axis"
ui_step "pick the axis line"               ui_click_logged "Sketch entity F2/c4"
ui_step "pitch 5"                          ui_type_in "Panel Surface Helix input pitch" "5"
ui_step "revolutions 4"                    ui_type_in "Panel Surface Helix input revolutions" "4"
ui_expect_new "Preview Surface Helix: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added surface helix of 0 profiles and 1 curves, 4 turns of 5" "the surface helix added"
ui_expect_log "(F6.b0): volume 0.000 mm3" "a surface body"
helicoid=$(sed -n 's/^New body \(.*\) (F6\.b0): volume.*/\1/p' "$UI_LOG" | tail -1)
[ -n "$helicoid" ] || ui_fail "the helicoid's body has no name in the log"
cylinder=$(sed -n 's/^New body \(.*\) (F3\.b0): volume.*/\1/p' "$UI_LOG" | tail -1)
[ -n "$cylinder" ] || cylinder=Body2

echo "--- Intersection Curve of the helicoid and Cylinder2, picked on their Browser rows"
ui_mark
ui_step "intersection curve (search)"      ui_command "Intersection Curve"
ui_expect_new "Command Intersection Curve started" "the command started"
ui_step "the first input"                  ui_click_logged "Panel Intersection Curve input first"
ui_step "the helicoid's row"               ui_click_logged "Browser Root/Bodies/$helicoid"
ui_expect_new "Intersection Curve First Curves: 1 " "the helicoid picked"
ui_step "the second input"                 ui_click_logged "Panel Intersection Curve input second"
ui_step "Cylinder2's row"                  ui_click_logged "Browser Root/Bodies/$cylinder"
ui_expect_new "Intersection Curve Second Curves: 1 " "the cylinder picked"
ui_expect_new "Preview Intersection Curve: ok" "previewed"
expect_curve "the helix on Cylinder2" "70.000, 0.000, 0.000" "70.000, 0.000, 20.000"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added intersection curve of 1 and 1 curve(s)" "the intersection curve added"
ui_expect_new "Construction shown: HelixCurve1, IntersectionCurve1" "both curves drawn"

echo "--- Pipe along the intersection curve"
ui_mark
ui_step "pipe (search)"                    ui_command "Pipe"
ui_step "section size 2"                   ui_type_in "Panel Pipe input size" "2"
ui_step "the path input"                   ui_click_logged "Panel Pipe input path"
ui_step "pick IntersectionCurve1"          ui_click_logged "Timeline IntersectionCurve1"
ui_expect_new "Pipe Path: 1 feature [feature F7]" "the intersection curve picked as the path"
ui_expect_new "Preview Pipe: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_volume "New body .* (F8.b0): volume \([0-9.]*\) mm3" "$PIPE" "a 2 mm pipe along the intersection" 1e-3

echo "--- The design file keeps the three"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $WORK/curves.mitcad" "saved"
expect_file "$WORK/curves.mitcad" \
  '[(f["type"], "radius" in f, f["pitch"] is not None) for f in doc["features"] if f["type"] == "helix_curve"]' \
  "[('helix_curve', False, True)]" "the helix curve takes the face's radius"
expect_file "$WORK/curves.mitcad" \
  '[(f.get("surface"), f["curves"]["sketch"]) for f in doc["features"] if f["type"] == "helix"]' \
  "[(True, 'F2')]" "the surface helix of Sketch1's line"
expect_file "$WORK/curves.mitcad" \
  '[(f["first"]["geometry"][0]["body"], f["second"]["geometry"][0]["body"]) for f in doc["features"] if f["type"] == "intersection_curve"]' \
  "[('F6.b0', 'F3.b0')]" "the intersection of the helicoid and Cylinder2"

echo "--- Each opens again in its own panel"
ui_mark
ui_step "double-click HelixCurve1"         ui_double_click_logged "Timeline HelixCurve1"
ui_expect_new "Editing F4 with Helix Curve" "editing HelixCurve1"
ui_expect_new "Preview Helix Curve: ok" "previewed as it is"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Helix Curve cancelled" "nothing changed"
ui_mark
ui_step "double-click Helix1"              ui_double_click_logged "Timeline Helix1"
ui_expect_new "Editing F6 with Surface Helix" "editing Helix1 with Surface Helix"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Surface Helix cancelled" "nothing changed"
ui_mark
ui_step "double-click IntersectionCurve1"  ui_double_click_logged "Timeline IntersectionCurve1"
ui_expect_new "Editing F7 with Intersection Curve" "editing IntersectionCurve1"
ui_expect_new "Preview Intersection Curve: ok" "previewed as it is"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Intersection Curve cancelled" "nothing changed"

echo "PASS: curve tools"
