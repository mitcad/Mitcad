#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/print app/MainWindowContextMenu.cpp app/commands/MaterialCommands.cpp app/commands/MaterialBinding.cpp core/model/src/document/bindings.rs tools/cli
# Bindings (mitcad#809, mitcad#861; the model's mitcad#800; the tools' fx
# since mitcad#1059) through the real UI:
#   - A design (mitcad-cli): Box1, Box2 and Box3 (Body1, Body2, Body3); the
#     text parameters Color ('Black', allowed values 'Black' and 'Red') and
#     Holes ('No'). No menu has Follow Option any more.
#   - Body1's Appearance… starts the Appearance tool; its fx on, Varies With
#     lists Color (allowed values) and not Holes; Color chosen: a row per
#     value, Black the value now; Paint - Black for Black and Paint - Red
#     for Red; OK writes switch(Color; ...).
#   - Edit Parameters: Color = 'Red' turns Body1 red (the saved design);
#     Body1's menu marks Appearance… with fx.
#   - Body2's Material ▸ More…: the Material tool binds the material to
#     Color the same way (ABS Plastic, PLA Plastic, Steel for any other
#     value).
#   - Body1's Appearance… again: the binding is shown (Red marked as the
#     value now); the fx off and OK keep Paint - Red as the appearance set
#     by hand, in one undo step; undo brings the binding back.
#   - Body1, Body2 and Body3 selected: the tool shows Body1's table; a row
#     changed and OK write the table to all three in one undo step, and
#     undo restores each.
#   - Box2 in the timeline: Suppress When... Holes == 'No' suppresses it;
#     Holes == 'Yes' in Edit Parameters brings it back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-bindings-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-bindings.XXXXXX)
FILE=$WORK/bindings.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# last_menu: the last "Context menu: ..." line since the mark.
last_menu() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Context menu: " | tail -1; }

# no_follow_option description: the last menu has no Follow Option entry.
no_follow_option() {
  ! last_menu | grep -qF "Follow Option" || ui_fail "$1 still has Follow Option: $(last_menu)"
  echo "ok   $1 has no Follow Option"
}

# expect_exact_since line description: that exact line in the log since the
# mark.
expect_exact_since() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qxF -- "$1" || ui_fail "$2: '$1' not in the log since the mark"
  echo "ok   $2"
}

# set_text name text: the parameter's expression in Edit Parameters,
# the table closed again.
set_text() {
  ui_mark
  ui_step "Edit Parameters (search)"     ui_command "Edit Parameters"
  ui_expect_new "Parameters dialog opened" "the table opened"
  ui_expect_log "Parameters $1 expression at" "$1's cells are logged"
  ui_focus_dialog '^Parameters$'
  ui_double_click_logged "Parameters $1 expression"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "'$2'"
  ui_key Return
  ui_expect_new "Parameter $1 expression: '$2' = " "$1 = '$2'"
  ui_step "OK (the table)"                 ui_click_logged "Parameters OK"
  ui_focus_main
}

SWITCH_1="switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_red')"

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-70, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_parameter", "name": "Color", "expression": "'Black'", "values": ["'Black'", "'Red'"]},
  {"cmd": "add_parameter", "name": "Holes", "expression": "'No'"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the boxes are in the view"

echo "--- No Follow Option in the menus"
ui_mark
ui_step "right-click Body1 in the view"    ui_click_logged "Body F1.b0" 3
ui_expect_new "Context menu: " "the view's menu"
last_menu | grep -qF "Appearance" || ui_fail "the view's menu has no Appearance...: $(last_menu)"
no_follow_option "the view's menu"
ui_key Escape
ui_focus_main

echo "--- Body1's appearance varies with Color (the Appearance tool)"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_expect_new "Context menu: " "the row's menu"
last_menu | grep -qF "Show When" || ui_fail "Show When... is not in the body's menu"
echo "ok   the body's menu has Show When..."
no_follow_option "the body's menu"
ui_step "choose Appearance..."             ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool for F1.b0" "the Appearance tool for Body1"
ui_expect_new "Panel Appearance input value_fx at" "the fx button is logged"
ui_step "fx"                               ui_click_logged "Panel Appearance input value_fx"
ui_expect_new "Appearance: Varies with an option = on" "the fx is on"
ui_expect_new "Panel Appearance options varies: None | Color" "Varies With lists Color (allowed values), not Holes"
ui_expect_new "Panel Appearance input varies at" "Varies With is logged"
ui_mark
ui_step "Varies With: Color"               ui_choose "Panel Appearance input varies" 1
ui_expect_new "Appearance tool varies with Color: 2 rows" "a row per value"
ui_expect_new "Panel Appearance label row0: Black (now)" "Black, the value now"
ui_expect_new "Panel Appearance label row1: Red" "Red"
ui_expect_new "Panel Appearance input row1 at" "the rows' places are logged"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Appearance switch("; then
  ui_fail "choosing Color wrote a binding"
fi
echo "ok   nothing written yet"
ui_mark
ui_step "Black: Paint - Black"             ui_choose_text "Panel Appearance input row0" "Paint - Bla"
ui_expect_new " = Paint - Black" "Paint - Black for Black"
ui_mark
ui_step "Red: Paint - Red"                 ui_choose_text "Panel Appearance input row1" "Paint - R"
ui_expect_new " = Paint - Red" "Paint - Red for Red"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Appearance $SWITCH_1 for 1 body(ies)" "the binding written"
save
expect_file '.bodies[0] | [.appearance, .appearance_expr]' "[\"paint_black\",\"$SWITCH_1\"]" \
  "Body1 is black and follows Color"

echo "--- Edit Parameters: Color = 'Red'"
set_text Color Red
save
expect_file '.bodies[0].appearance' '"paint_red"' "Body1 turned red"

echo "--- Body2's material varies with Color (the Material tool)"
ui_mark
ui_step "right-click Body2"                ui_click_logged "Browser Root/Bodies/Body2" 3
ui_expect_new "Context menu: " "the row's menu"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Context menu fx"; then
  ui_fail "Body2's menu marks a binding it does not have"
fi
echo "ok   no fx mark without a binding"
ui_step "choose Material > More..."        ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for bodies F2.b0" "the Material tool for Body2"
ui_expect_new "Panel Material bodies note: 1 body: Steel" "the panel notes what Body2 is made of"
ui_step "fx"                               ui_click_logged "Panel Material input material_fx"
ui_expect_new "Panel Material input varies at" "Varies With is logged"
ui_mark
ui_step "Varies With: Color"               ui_choose "Panel Material input varies" 1
ui_expect_new "Material tool varies with Color: 2 rows" "a row per value"
ui_expect_new "Panel Material label row1: Red (now)" "Red is the value now"
ui_expect_new "Panel Material input row1 at" "the rows' places are logged"
ui_mark
ui_step "Black: ABS Plastic"               ui_choose_text "Panel Material input row0" "ABS"
ui_expect_new " = ABS Plastic" "ABS Plastic for Black"
ui_mark
ui_step "Red: PLA Plastic"                 ui_choose_text "Panel Material input row1" "PLA P"
ui_expect_new " = PLA Plastic" "PLA Plastic for Red"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material tool: varies with Color to 1 bodies: switch(Color; 'Black'; 'abs'; 'Red'; 'pla'; 'steel')" \
  "the binding written"
save
expect_file '.bodies[1] | [.material, .material_expr]' \
  "[\"pla\",\"switch(Color; 'Black'; 'abs'; 'Red'; 'pla'; 'steel')\"]" "Body2 is PLA and follows Color"

echo "--- Body1: the fx off keeps the appearance shown; undo brings the binding back"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_expect_new "Context menu fx: Appearance..." "the menu marks Appearance... (fx)"
ui_step "choose Appearance..."             ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool: the targets follow $SWITCH_1" "the tool shows Body1's binding"
ui_expect_new "Panel Appearance label row1: Red (now)" "Red is marked as the value now"
ui_expect_new "Panel Appearance tile value: Paint - Red (" "the picker shows what Body1 has now"
ui_mark
ui_step "fx off"                           ui_click_logged "Panel Appearance input value_fx"
ui_expect_new "Appearance: Varies with an option = off" "the fx is off"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Appearance paint_red for 1 body(ies)" "the shown appearance set by hand"
save
expect_file '.bodies[0] | [.appearance, .appearance_expr]' '["paint_red",null]' \
  "Body1 keeps red, the binding gone"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Appearance of Body1" "one undo step"
save
expect_file '.bodies[0] | [.appearance, .appearance_expr]' "[\"paint_red\",\"$SWITCH_1\"]" \
  "undo brings the binding back"

echo "--- Three bodies of different assignments: one table written to all"
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Browser selected: Root/Bodies/Body1" "Body1 selected"
ui_step "Ctrl+click Body2"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body2") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2" "Body1 and Body2 selected"
ui_step "Ctrl+click Body3"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body3") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2, Root/Bodies/Body3" "the three bodies selected"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."             ui_menu_choose "Appearance..."
ui_expect_new "Appearance tool: the targets follow $SWITCH_1" "the table shows Body1's state"
ui_expect_new "Panel Appearance input row0 at" "the rows' places are logged"
ui_mark
ui_step "Black: Paint - White"             ui_choose_text "Panel Appearance input row0" "Paint - W"
ui_expect_new " = Paint - White" "Paint - White for Black"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Appearance switch(Color; 'Black'; 'paint_white'; 'Red'; 'paint_red') for 3 body(ies)" \
  "written to all three"
save
expect_file '[.bodies[] | .appearance_expr]' \
  "[\"switch(Color; 'Black'; 'paint_white'; 'Red'; 'paint_red')\",\"switch(Color; 'Black'; 'paint_white'; 'Red'; 'paint_red')\",\"switch(Color; 'Black'; 'paint_white'; 'Red'; 'paint_red')\"]" \
  "the three bodies follow the shown table"
expect_file '[.bodies[] | .appearance]' '["paint_red","paint_red","paint_red"]' "all three are red (Color = 'Red')"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Appearance" "one undo step"
save
expect_file '[.bodies[] | .appearance_expr]' "[\"$SWITCH_1\",null,null]" "undo restores each body's state"

echo "--- Box2: Suppress When... Holes == 'No'"
ui_mark
ui_step "right-click Box2"                 ui_click_logged "Timeline Box2" 3
ui_step "choose Suppress When..."          ui_menu_choose "Suppress When..."
ui_focus_dialog '^Suppress Box2 When$'
ui_expect_new "Condition dialog Suppress Box2 When: none" "the dialog opened without a condition"
ui_expect_new "Condition OK at" "its OK is logged"
ui_step "type Holes == 'No'"                ui_type_text "Holes == 'No'"
ui_mark
ui_step "OK"                               ui_click_logged "Condition OK"
ui_focus_main
ui_expect_new "Suppress Box2 when Holes == 'No'" "Box2 follows the condition"
save
expect_file '.features[1] | [.suppressed, .suppressed_when]' "[true,\"Holes == 'No'\"]" "Box2 is suppressed"
set_text Holes Yes
save
expect_file '.features[1] | [(.suppressed // false), .suppressed_when]' "[false,\"Holes == 'No'\"]" \
  "Box2 is back, still following the condition"

ui_finish "UI bindings"
