#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp core/model/src/document/bindings.rs tools/cli
# Bindings at a glance (mitcad#862) through the real UI, on a design
# mitcad-cli makes:
#   - Body1 (an extrusion, F2.b0): its appearance follows switch(Color),
#     it is shown when Holes == 'No', and its side face's appearance follows
#     if(Color = 'Black'; ...): three bindings.
#   - Body2 (Box1, F3.b0): its appearance follows switch(Color) with a
#     result that names no appearance ('paint_purple').
#   - Box2 (F4): suppressed when Holes == 'Yes' (not suppressed now).
#   - Board:1, an occurrence of the component Board: shown when
#     Color <> 'Red'.
#   - The options Color (Black, Red) and Holes (No, Yes) of the
#     configuration.
# Steps:
#   1. Body1's row has the fx badge `fx 3`; resting the pointer on it shows
#      the card, which closes when the pointer leaves; a click pins it: the
#      title with the options' values now and one readable line per
#      property (the current case marked ●, the condition with "shown now",
#      the face's expression with what it gives); Esc closes it. Edit… of
#      Appearance opens the Appearance tool for Body1 with its binding
#      (mitcad#1059).
#   2. Board:1's card (an occurrence), and Box2's from the timeline's badge
#      (a feature suppressed by a condition); its link opens Configurations
#      on the Bindings tab.
#   3. The Bindings tab lists the six bindings (bodies, a face, a feature, an
#      occurrence), warns about 'paint_purple', filters by Option
#      (mitcad#968) and by the item search; the Options tab says how many
#      bindings use each
#      option; Select in Browser shows Box2; Remove Binding of Body2's
#      appearance keeps paint_black in one undo step, and undo brings the
#      binding back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-bindings-glance-test.sh [screenshot folder]

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-bindings-glance.XXXXXX)
FILE=$WORK/glance.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
SIDE='F2:side(c1[c4,c2])'

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_focus_main
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'Black'",
   "values": ["'Black'", "'Red'"]},
  {"cmd": "add_parameter", "name": "Holes", "unit": "Text", "expression": "'No'",
   "values": ["'No'", "'Yes'"]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-60, -10], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-30, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F2.b0",
   "expression": "switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_red'; 'paint_black')"},
  {"cmd": "set_body_visible", "uid": "F2.b0", "expression": "Holes == 'No'"},
  {"cmd": "set_face_appearance", "uid": "F2.b0", "faces": ["$SIDE"],
   "expression": "if(Color = 'Black'; 'paint_white'; 'paint_yellow')"},
  {"cmd": "set_body_appearance", "uid": "F3.b0",
   "expression": "switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_purple'; 'paint_black')"},
  {"cmd": "suppress_feature", "uid": "F4", "expression": "Holes == 'Yes'"},
  {"cmd": "create_component", "name": "Board"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -10],
    "length": 20, "width": 20, "height": 5, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "set_occurrence_visible", "occurrence": "Board:1", "expression": "Color <> 'Red'"},
  {"cmd": "set_configurations", "configurations": {"options": ["Color", "Holes"], "closed": false, "rows": []}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser fx badge Root/Bodies/Body1: fx 3" "Body1's fx badge counts its three bindings"
ui_expect_log "Browser fx badge Root/Bodies/Body2: fx 1" "Body2's badge: one binding"
ui_expect_log "Browser fx badge Root/Board:1: fx 1" "the occurrence's badge"
ui_expect_log "Browser fx badge Root/Bodies/Body1 at" "the badge's place is logged"

echo "--- 1. Body1's card"
ui_mark
ui_step "rest the pointer on Body1's badge" xdotool mousemove $(ui_logged_at "Browser fx badge Root/Bodies/Body1")
ui_expect_new "Bindings card shown for Body1 (hover)" "the card shows while the pointer rests on the badge"
ui_mark
# The view's top right corner: the card opens below the badge and is wide
# (a face's line), so it covers the middle of the view, where the pointer
# would keep it open.
ui_step "move the pointer off the card"    xdotool mousemove $(ui_view_at 95 5)
ui_expect_new "Bindings card closed" "the card closes when the pointer leaves the badge and the card"

ui_mark
ui_step "click Body1's badge"              ui_click_logged "Browser fx badge Root/Bodies/Body1"
ui_expect_new "Browser fx click Root/Bodies/Body1" "the click is on the badge"
ui_expect_new "Bindings card shown for Body1 (pinned)" "the click pins the card"
ui_expect_new "Bindings card Body1 follows; now: Color = Black, Holes = No" \
  "the title and the options' values now"
ui_expect_new "Bindings card line Appearance: Color: Black → Paint - Black ●, Red → Paint - Red" \
  "the appearance as cases of Color, the current one marked, no otherwise for a listed option"
ui_expect_new "Bindings card line Shown: when Holes = No · shown now" "Show When as a condition, shown now"
ui_expect_new "Bindings card line Face 1 (side): Color: Black → Paint - White ●, Red → Paint - Yellow" \
  "the face by a short label, its if() read as cases of Color"
ui_expect_new "Bindings card line Material: " "the material set by nothing, plainly"
ui_expect_new "Bindings card Edit Appearance at" "the Edit... buttons' places are logged"
ui_expect_new "Bindings card all bindings at" "the link's place is logged"
[ -n "$SHOTS" ] && ui_capture "$SHOTS/bindings-card.png"
ui_mark
ui_step "Esc"                              ui_key Escape
ui_expect_new "Bindings card closed" "Esc closes the pinned card"

ui_mark
ui_step "click Body1's badge again"        ui_click_logged "Browser fx badge Root/Bodies/Body1"
ui_expect_new "Bindings card Edit Appearance at" "the card is pinned again"
ui_mark
ui_step "Edit... of Appearance"            ui_click_logged "Bindings card Edit Appearance"
ui_expect_new "Bindings card Edit Appearance of Body1" "Edit... of the appearance"
ui_expect_new "Appearance tool for F2.b0" "the Appearance tool for Body1"
ui_expect_new "Appearance tool: the targets follow switch(Color; " "with Body1's binding"
ui_step "Cancel (Esc)"                     ui_key Escape

echo "--- 2. An occurrence's card, a feature's from the timeline"
ui_mark
ui_step "click Board:1's badge"            ui_click_logged "Browser fx badge Root/Board:1"
ui_expect_new "Bindings card shown for Board:1 (pinned)" "the occurrence's card"
ui_expect_new "Bindings card line Shown: when Color ≠ Red · shown now" "Board:1 is shown while Color is not Red"
ui_step "Esc"                              ui_key Escape
ui_focus_main
ui_expect_log "Timeline fx Box2 at" "Box2's badge in the timeline is logged"
ui_mark
ui_step "click Box2's badge in the timeline" ui_click_logged "Timeline fx Box2"
ui_expect_new "Timeline fx click Box2" "the click is on the timeline's badge"
ui_expect_new "Bindings card shown for Box2 (pinned)" "the feature's card"
ui_expect_new "Bindings card line Suppressed: when Holes = Yes · not suppressed now" \
  "Box2 is suppressed when Holes is Yes, not now"
ui_expect_new "Bindings card all bindings at" "the link's place is logged"
ui_mark
ui_step "All bindings of the design"       ui_click_logged "Bindings card all bindings"
ui_expect_new "Bindings card all bindings" "the link was clicked"
ui_expect_new "Configurations window opened" "Configurations opened"
ui_expect_new "Configurations page Bindings" "on the Bindings tab"

echo "--- 3. The Bindings tab"
ui_expect_new "Configurations binding Body1 Appearance: Color: Black → Paint - Black, Red → Paint - Red; now Paint - Black" \
  "Body1's appearance"
ui_expect_new "Configurations binding Body1 Shown: when Holes = No; now shown" "Body1's Show When"
ui_expect_new "Configurations binding Body1 Face 1 (side): Color: Black → Paint - White, Red → Paint - Yellow; now Paint - White" \
  "Body1's face"
ui_expect_new "Configurations binding Body2 Appearance: Color: Black → Paint - Black, Red → paint_purple; now Paint - Black; warning no appearance “paint_purple”" \
  "Body2's appearance, with a warning about the result that names nothing"
ui_expect_new "Configurations binding Box2 Suppressed: when Holes = Yes; now not suppressed" "the feature"
ui_expect_new "Configurations binding Board:1 Shown: when Color ≠ Red; now shown" "the occurrence"
ui_expect_new "Configurations bindings status: 6 bindings, 1 warning" "the status line counts them"
ui_expect_new "Configurations option Color used by 4 bindings" "Options: Color is used by four bindings"
ui_expect_new "Configurations option Holes used by 2 bindings" "Options: Holes by two"
ui_expect_new "Configurations bindings driven by at" "the Option filter's place is logged"
ui_expect_log "Configurations bindings filter label: Option:" "the filter is labelled Option (mitcad#968)"
ui_expect_new "Configurations binding Body2 Appearance at" "the rows' places are logged"
[ -n "$SHOTS" ] && ui_capture "$SHOTS/bindings-tab.png"

ui_mark
ui_focus_dialog '^Configurations$'
ui_step "Option: Holes"                    ui_choose "Configurations bindings driven by" 2
ui_expect_new "Configurations bindings driven by Holes, search '': 2 of 6 bindings" "the bindings Holes drives"
ui_mark
ui_step "Option: All options"              ui_choose "Configurations bindings driven by" -2
ui_expect_new "Configurations bindings driven by All options, search '': 6 bindings, 1 warning" "all again"
ui_mark
ui_step "search Board"                     ui_type_in "Configurations bindings search" "Board"
ui_expect_new "Configurations bindings driven by All options, search 'Board': 1 of 6 bindings" \
  "the item search finds the occurrence"
ui_mark
ui_step "clear the search"                 ui_key ctrl+a BackSpace
ui_expect_new "Configurations bindings driven by All options, search '': 6 bindings, 1 warning" "all again"

ui_mark
ui_step "select Box2's row"                ui_click_logged "Configurations binding Box2 Suppressed"
ui_step "Select in Browser"                ui_click_logged "Configurations bindings select"
ui_expect_new "Configurations binding select: Box2 Suppressed" "Select in Browser"
ui_expect_new "Revealed Box2" "Box2 is selected and shown"

ui_mark
ui_focus_dialog '^Configurations$'
ui_step "select Body2's appearance"        ui_click_logged "Configurations binding Body2 Appearance"
ui_expect_new "Configurations bindings selected warning: no appearance “paint_purple”" \
  "the selected row's warning is in the status line"
ui_step "Remove Binding"                   ui_click_logged "Configurations bindings remove"
ui_expect_new "Configurations binding removed: Body2 Appearance" "the binding is removed"
ui_expect_new "Configurations bindings status: 5 bindings" "five are left"
save
expect_file '.bodies[] | select(.uid == "F3.b0") | [.appearance, .appearance_expr]' '["paint_black",null]' \
  "Body2 keeps the appearance the binding gave"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Clear Appearance Expression of Body2" "one undo step"
ui_expect_new "Configurations bindings status: 6 bindings, 1 warning" "the tab lists it again"
save
expect_file '.bodies[] | select(.uid == "F3.b0") | (.appearance_expr | startswith("switch(Color;"))' 'true' \
  "undo brings the binding back"

ui_finish "UI bindings at a glance"
