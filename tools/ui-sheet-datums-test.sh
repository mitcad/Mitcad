#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/commands/CommandSupport.cpp app/OcctViewer.cpp
# check-all sources: geometry/src/datum.cpp core/model/src/features/construction core/model/src/sheet tools/cli
# Construction geometry on the edges of sheet bodies through the real UI
# (mitcad#1537). Design (mitcad-cli, as cli.sheet_datums): rules t = 2,
# r = 3, K = 0.3, the reference side the front;
#   - a 40 x 20 x 2 plate converted to a sheet of one panel (F2.b0, its
#     front at z = 2), whose edges are lines;
#   - a 40 x 30 x 20 block at x = 100 converted to a U channel of its bottom
#     and end faces (F4.b0);
#   - a 60 x 20 Face at y = 50..70 folded 90 degrees up at x = 0 (F7.b0).
# The channel's and the folded sheet's straight edges are B-spline curves
# (the sheet's paired rails). Before mitcad#1537 a pick called them no line
# and the Line inputs refused them (OK stayed refused), and the model took
# them for no line either; the flat sheet worked, a fold broke it.
#   1. Plane Through Two Edges on two front edges of the plate (x = 0 along
#      +y, y = 0 along +x): the plane z = 2 through (0, 0, 2), normal
#      d1 x d2 = (0, 0, -1).
#   2. Plane Through Two Edges on the channel's outer face at x = 140 (its
#      top edge along -y, its edge at y = 0 along -z): the plane x = 140
#      through (140, 30, 20), normal (1, 0, 0).
#   3. Point Through Two Edges on the same two edges: (140, 0, 20).
#   4. Plane Through Two Edges on the folded upright's front top edge
#      (along -y) and its front edge at y = 70 (along +z): the plane
#      x = 5 - 0.9 pi = 2.17257 through (2.17257, 70, 32.1726), normal
#      (-1, 0, 0) (the fold's allowance (pi/2)(r + K t), half of it 0.9 pi).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-datums-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sheet-datums.XXXXXX)
FILE=$WORK/sheets.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":2,"bend_radius":3,"allowance":{"type":"k_factor","k":0.3},"reference_side":"front","bend_relief":{"type":"rectangular","width":2,"depth":3},"corner_relief":{"type":"round","width":2,"depth":3}}},
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":40,"width":20,"height":2,"operation":"new_body"}},
  {"cmd":"add_feature","def":{"type":"sheet_convert","rules":"F1","body":"F2.b0","face":"F2:top"}},
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[100,0],"length":40,"width":30,"height":20,"operation":"new_body"}},
  {"cmd":"add_feature","def":{"type":"sheet_convert","rules":"F1","body":"F4.b0","face":"F4:bottom","bends":["E{F4:bottom|F4:side1}","E{F4:bottom|F4:side3}"]}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F6","corner":[-30,50],"width":60,"height":20},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F6","region":"$RECT"}]}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_line","sketch":"F8","start":[0,50],"end":[0,70]},
  {"cmd":"add_feature","def":{"type":"sheet_fold","body":"F7.b0","panel":{"feature":"F7","slot":0},"line":{"sketch":"F8","curve":"c1"},"stationary_side":"left","angle":1.5707963267948966,"position":"bend_line"}},
  {"expect":{"feature":"F9","status":"ok"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

PLATE_X0='edge F2.b0/E{F3:sheet_front(0)|F3:sheet_side0(F3:sheet_curve(1000000))}'
PLATE_Y0='edge F2.b0/E{F3:sheet_front(0)|F3:sheet_side0(F3:sheet_curve(1000003))}'
WALL_TOP='edge F4.b0/E{F5:sheet_front(1)|F5:sheet_side1(F5:sheet_curve(1000005))}'
WALL_END='edge F4.b0/E{F5:sheet_front(1)|F5:sheet_side1(F5:sheet_curve(1000004))}'
UPRIGHT_TOP='edge F7.b0/E{F9:sheet_front(0)|F9:sheet_side0(F6:sheet_curve(c2[c1,c3]))}'
UPRIGHT_SIDE='edge F7.b0/E{F9:sheet_front(0)|F9:sheet_side0(F9:sheet_curve(50))}'

# two_edges command line1 line2: picks two edges in a fresh command and
# checks that each was taken and the preview made.
two_edges() {
  ui_mark
  ui_step "$1 (search)"                    ui_command "$1"
  ui_step "pick line 1"                    ui_click_pick "$2"
  ui_expect_new "$1 Line 1: 1 edge" "line 1 taken"
  ui_step "pick line 2"                    ui_click_pick "$3"
  ui_expect_new "$1 Line 2: 1 edge" "line 2 taken"
  ui_expect_new "Preview $1: ok" "previewed"
  ui_step "OK (Enter)"                     ui_key Return
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- 1. Plane Through Two Edges on the converted plate"
two_edges "Plane Through Two Edges" "$PLATE_X0" "$PLATE_Y0"
ui_expect_new "Added plane Plane1: origin (0, 0, 2), normal (0, 0, -1)" "Plane1 on the plate's front"

echo "--- 2. Plane Through Two Edges on the channel's B-spline edges"
two_edges "Plane Through Two Edges" "$WALL_TOP" "$WALL_END"
ui_expect_new "Added plane Plane2: origin (140, 30, 20), normal (1, 0, 0)" "Plane2 on the wall's outer face"

echo "--- 3. Point Through Two Edges on the same channel edges"
two_edges "Point Through Two Edges" "$WALL_TOP" "$WALL_END"
ui_expect_new "Added point Point1: point (140, 0, 20)" "Point1 at the wall's corner"

echo "--- 4. Plane Through Two Edges on the folded sheet's upright"
two_edges "Plane Through Two Edges" "$UPRIGHT_TOP" "$UPRIGHT_SIDE"
ui_expect_new "Added plane Plane3: origin (2.17257, 70, 32.1726), normal (-1, 0, 0)" "Plane3 on the upright's front"
[ -n "$SHOT" ] && ui_capture "$SHOT"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI sheet datums test"
