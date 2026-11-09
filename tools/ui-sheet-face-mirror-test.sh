#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PatternCommands.cpp app/commands/CommandSupport.cpp app/OcctViewer.cpp
# check-all sources: geometry/src/datum.cpp core/model/src/features/mirror.rs core/model/src/sheet tools/cli
# Mirror about a flat face of a sheet body through the real UI
# (mitcad#1554). Design (mitcad-cli, as cli.sheet_face_planes): rules
# t = 2, r = 3, K = 0.3, the reference side the front;
#   - a 40 x 20 x 2 plate converted to a sheet of one panel (F2.b0, its
#     front at z = 2);
#   - a 60 x 20 Face at y = 50..70 folded 90 degrees up at x = 0 (F5.b0),
#     the top of its upright at z = 35 - 0.9 pi = 32.1726 (the fold's
#     allowance (pi/2)(r + K t), half of it 0.9 pi), a B-spline face;
#   - a 4 x 3 x 5 block at x = 60..64, y = -40..-37, z = 0..5 (F8.b0).
# A sheet's thickness walls are B-spline surfaces once a bend is in it,
# planar within 1e-6 mm. Before mitcad#1537 a pick called such a wall no
# plane, so the Mirror Plane input refused it; the model took it (the
# image's placement: cli.sheet_face_planes).
#   1. Mirror of the block about the plate's front: a new body of 60 mm3.
#   2. Mirror of the block about the folded upright's top (B-spline): a new
#      body of 60 mm3.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-face-mirror-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sheet-face-mirror.XXXXXX)
FILE=$WORK/sheets.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":2,"bend_radius":3,"allowance":{"type":"k_factor","k":0.3},"reference_side":"front","bend_relief":{"type":"rectangular","width":2,"depth":3},"corner_relief":{"type":"round","width":2,"depth":3}}},
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":40,"width":20,"height":2,"operation":"new_body"}},
  {"cmd":"add_feature","def":{"type":"sheet_convert","rules":"F1","body":"F2.b0","face":"F2:top"}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F4","corner":[-30,50],"width":60,"height":20},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F4","region":"$RECT"}]}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_line","sketch":"F6","start":[0,50],"end":[0,70]},
  {"cmd":"add_feature","def":{"type":"sheet_fold","body":"F5.b0","panel":{"feature":"F5","slot":0},"line":{"sketch":"F6","curve":"c1"},"stationary_side":"left","angle":1.5707963267948966,"position":"bend_line"}},
  {"expect":{"feature":"F7","status":"ok"}},
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[60,-40],"length":4,"width":3,"height":5,"operation":"new_body"}},
  {"expect":{"body":"F8.b0","volume":60}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

PLATE_FRONT='F2.b0/F3:sheet_front(0)'
UPRIGHT_TOP='F5.b0/F7:sheet_side0(F4:sheet_curve(c2[c1,c3]))'

# mirror_about face: Mirror of the block about a face, picked in the view.
mirror_about() {
  ui_mark
  ui_step "mirror (search)"                ui_command "Mirror"
  ui_step "pick the block"                 ui_click_logged "Body F8.b0"
  ui_step "the plane input"                ui_click_logged "Panel Mirror input plane"
  ui_step "pick the face"                  ui_click_pick "face $1"
  ui_expect_new "Mirror Mirror Plane: 1 face [face ${1#*/} of ${1%%/*}" "the face taken as the plane"
  ui_expect_new "Preview Mirror: ok" "previewed"
  ui_step "OK (Enter)"                     ui_key Return
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- 1. Mirror about the converted plate's front"
mirror_about "$PLATE_FRONT"
ui_expect_volume "New body [A-Za-z0-9]* (F9.b0): volume \([0-9.]*\) mm3" 60 "the image about the plate's front"

echo "--- 2. Mirror about the folded upright's top (a B-spline face)"
mirror_about "$UPRIGHT_TOP"
ui_expect_volume "New body [A-Za-z0-9]* (F10.b0): volume \([0-9.]*\) mm3" 60 "the image about the upright's top"
[ -n "$SHOT" ] && ui_capture "$SHOT"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI sheet face mirror test"
