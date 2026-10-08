#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Hide Above Sketch, the sketch palette's option, through the real UI:
#   - Body1 from z = 0 to 10 and Body2 from z = 5 to 15; a sketch on the
#     construction plane at z = 5 has 5 mm of Body1 and all of Body2 in
#     front of it.
#   - Hide Above Sketch cuts the bodies at the sketch plane: nothing is in
#     front of it, Body2 is gone from the view; off again, both are whole.
#   - The cut face of Body1 is shown as a section cap (hatched); Body3,
#     from z = 0 to 5, only touches the plane: its top face is its own, no
#     cap.
#   - The option stays for the next sketch, and the bodies are cut only
#     while sketching.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-hide-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-hide.XXXXXX)
FILE=$WORK/blocks.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# Body1 (F2.b0) 30 x 20 x 10 on XY; Plane1 (F3) 5 above XY; Body2 (F5.b0)
# 10 x 10 x 10 on Plane1, beside Body1; Body3 (F7.b0) 10 x 10 x 5 on XY,
# behind Body1.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 5}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [40, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [0, 30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F6", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0, F5.b0, F7.b0" "the bodies shown"

echo "--- A sketch on Plane1"
ui_create_sketch F3
ui_expect_log "Palette paletteHideAbove at" "the option is in the palette"
ui_expect_log "In front of the sketch plane: F2.b0 5.0, F5.b0 10.0" "5 mm of Body1 and Body2 in front"

echo "--- Hide Above Sketch"
ui_mark
ui_step "check Hide Above Sketch"          ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch on" "on"
ui_expect_new "In front of the sketch plane: F2.b0 0.0" "Body1 cut at the plane, Body2 gone"
ui_expect_new "Section caps: 1 face(s)" "Body1's cut face as a cap, not Body3's top"
if [ -n "$SHOT" ]; then
  ui_sync
  ui_capture "$SHOT"
fi
ui_mark
ui_step "uncheck Hide Above Sketch"        ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch off" "off"
ui_expect_new "In front of the sketch plane: F2.b0 5.0, F5.b0 10.0" "the bodies whole again"

echo "--- The next sketch"
ui_step "check Hide Above Sketch"          ui_click_logged "Palette paletteHideAbove"
ui_expect_log "Hide Above Sketch on" "on again"
ui_mark
ui_step "finish the sketch (Ctrl+Enter)"   ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch left"
ui_create_sketch F3
ui_expect_new "In front of the sketch plane: F2.b0 0.0" "the next sketch hides above too"

ui_finish "UI sketch hide test"
