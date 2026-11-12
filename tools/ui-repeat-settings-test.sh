#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/MainWindowContextMenu.cpp tools/cli
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSession.hpp
# check-all sources: app/framework/CommandSessionAuto.cpp app/framework/DocumentSession.hpp
# check-all sources: app/commands/MaterialCommands.cpp
# Repeat starts the last command with the settings of its last OK
# (mitcad#1686) through the real UI, on three boxes side by side (F1.b0,
# F2.b0, F3.b0) and a copy of Paint - Red in the design:
#   1. Body1's row: Appearance…, Red Copy chosen in the picker, OK: Body1
#      has it.
#   2. Enter with no command running: Repeat Appearance, its panel with
#      Red Copy already chosen and no body; a click on Body2 and OK give
#      Body2 the same appearance without choosing it again.
#   3. A face of Body3 selected, Repeat Appearance in the view's context
#      menu: the panel with Red Copy and that face; OK gives the face it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-repeat-settings-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-repeat-settings.XXXXXX")
FILE=$WORK/boxes.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# expect_file filter expected description: a jq filter on the saved project
# file.
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

# Three 40 x 20 x 20 boxes 10 mm apart along X, and a copy of Paint - Red
# in the design.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-75, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [35, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_appearance", "id": "red_copy", "name": "Red Copy", "based_on": "paint_red"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F2.b0 at" "the second box is in the view"
ui_expect_log "Body F3.b0 at" "and the third"

echo "--- 1. Appearance of Body1: Red Copy"
ui_mark
ui_step "right-click Body1's row"        ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Appearance..."           ui_menu_choose "Appearance..."
ui_expect_new "Appearance Bodies: 1 body [body F1.b0]" "Body1 in its Bodies"
ui_step "the picker: Red Copy"           ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red Copy"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance red_copy for 1 body(ies)" "Body1 got Red Copy"

echo "--- 2. Enter repeats Appearance with Red Copy; Body2 picked, OK"
ui_sync
ui_mark
ui_step "Enter (no command running)"     ui_key Return
ui_expect_new "Repeat Appearance (Enter)" "Enter repeats Appearance"
ui_expect_new "Command Appearance started" "Appearance started again"
ui_expect_new "Panel Appearance repeated: value=red_copy" "its panel starts with Red Copy"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Appearance Bodies: 1 body"; then
  ui_fail "the repeated Appearance started with a body"
fi
echo "ok   no body picked yet"
read -r cx cy <<< "$(ui_logged_at "Body F2.b0")"
# The logged place is where the body's corner nearest the camera is: its top
# face is above it.
ui_step "click Body2"                    xdotool mousemove "$cx" $((cy - 20)) click 1
ui_expect_new "Appearance Bodies: 1 body [body F2.b0]" "Body2 in its Bodies"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance red_copy for 1 body(ies)" "Body2 got Red Copy without choosing it again"
save
expect_file '[.bodies[] | .appearance]' '["red_copy","red_copy",null]' "Body1 and Body2 have Red Copy, Body3 not"

echo "--- 3. Repeat in the context menu, on a face of Body3"
ui_sync
read -r dx dy <<< "$(ui_logged_at "Body F3.b0")"
ui_mark
ui_step "click the top face of Body3"    xdotool mousemove "$dx" $((dy - 20)) click 1
ui_expect_new "Selected: 1 face [face" "a face of Body3 selected"
ui_mark
ui_step "right-click the selected face"  xdotool mousemove "$dx" $((dy - 20)) click 3
ui_step "choose Repeat Appearance"       ui_menu_choose "Repeat Appearance"
ui_expect_new "Command Appearance started" "Appearance started from the context menu"
ui_expect_new "Panel Appearance repeated: value=red_copy" "with Red Copy again"
ui_expect_new "Appearance Faces: 1 face" "and the selected face in its Faces"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance red_copy for 0 body(ies), 1 face(s)" "the face got Red Copy"
save
expect_file '[.bodies[] | has("face_appearances")]' '[false,false,true]' "only Body3's face has one of its own"

ui_finish "UI Repeat with the last settings"
