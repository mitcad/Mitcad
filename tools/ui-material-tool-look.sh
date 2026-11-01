#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Material and Appearance tools (mitcad#1059): a one-off visual check,
# not a recurring test (its name is not ui-*-test.sh, so no check picks it
# up). On three boxes, a material of the design's own, a user library
# with Shop PETG and the text parameter Color, it saves pictures of the
# Qt frontend on the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME):
#   <dir>/material-tool-<theme>-panel.png      the Material tool for two
#                                              bodies: the picker button,
#                                              the fx, Appearance, Bodies
#                                              with what they are made of
#   <dir>/material-tool-<theme>-picker.png     its picker open
#   <dir>/material-tool-<theme>-fx.png         the fx on, Varies With Color,
#                                              a row per value
#   <dir>/material-tool-<theme>-appearance.png the Appearance tool with its
#                                              picker open
# The app's log as <dir>/look-<theme>.log. The native (AppKit) pictures
# are taken on a Mac.
#
# What to look for: the picker button's swatch, name and the line under
# it; the fx button beside it, pressed when on; the picker's search, its
# headings and swatches; the rows named by the values with Black (now);
# the text readable on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-material-tool-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-material-tool-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
export MITCAD_PRINT_LIBRARY_DIR=$DIR/print-library
mkdir -p "$MITCAD_PRINT_LIBRARY_DIR/materials"
printf '%s\n' '{"format": "mitcad-print-item", "version": 1, "material": {"id": "shop_petg",' \
  '"name": "Shop PETG", "density": 1.27, "appearance": "plastic_black", "version": 1}}' \
  > "$MITCAD_PRINT_LIBRARY_DIR/materials/shop_petg.json"
FILE=$DIR/boxes.mitcad
printf '%s\n' '[' \
  '{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10], "length": 40, "width": 20, "height": 20, "operation": "new_body"}},' \
  '{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10], "length": 40, "width": 20, "height": 20, "operation": "new_body"}},' \
  '{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [55, -10], "length": 40, "width": 20, "height": 20, "operation": "new_body"}},' \
  '{"cmd": "create_material", "id": "acme_petg", "name": "Acme PETG", "density": 1.27, "appearance": "plastic_orange"},' \
  '{"cmd": "set_body_material", "uid": "F2.b0", "material": "acme_petg"},' \
  '{"cmd": "add_parameter", "name": "Color", "expression": "'"'"'Black'"'"'", "values": ["'"'"'Black'"'"'", "'"'"'Red'"'"'"]}' \
  ']' > "$DIR/model.json"
"$CLI" run "$DIR/model.json" --save "$FILE" > "$DIR/cli.log" 2>&1 || { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }

# shot name: the whole screen as <dir>/material-tool-<name>.png.
shot() {
  ui_wait_idle
  ui_sync
  sleep 2
  xwd -root -silent > "$DIR/screen.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/screen.xwd" "$DIR/material-tool-$1.png"
  echo "ok   $DIR/material-tool-$1.png"
}

ui_start_display
for theme in light dark; do
  echo "--- The $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --open "$FILE" --no-native-dialogs
  ui_expect_log "Window style: $theme, " "the window's style is $theme"
  ui_step "fit (F6)"                         ui_key F6
  ui_mark
  ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
  ui_step "Ctrl+click Body2"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body2") keydown ctrl click 1 keyup ctrl
  ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2" "Body1 and Body2 selected"
  ui_mark
  ui_step "Material (search)"                ui_command "Material"
  ui_expect_new "Panel Material bodies note: " "the panel notes what the bodies are made of"
  ui_expect_new "Panel Material input material at" "the picker button's place"
  xdotool mousemove 5000 5000
  shot "$theme-panel"
  ui_mark
  ui_step "open the picker"                  ui_click_logged "Panel Material input material"
  ui_expect_new "Material picker opened: " "the picker opened"
  ui_focus_dialog '^Material Picker$'
  xdotool type --delay 40 "PET"
  ui_expect_new "Material picker search: PET, first " "the search narrowed the list"
  shot "$theme-picker"
  ui_step "close the picker (Escape)"        ui_key Escape
  ui_focus_main
  ui_mark
  ui_step "fx"                               ui_click_logged "Panel Material input material_fx"
  ui_expect_new "Panel Material input varies at" "Varies With shows"
  ui_step "Varies With: Color"               ui_choose "Panel Material input varies" 1
  ui_expect_new "Material tool varies with Color: 2 rows" "a row per value"
  ui_step "Black: ABS Plastic"               ui_choose_text "Panel Material input row0" "ABS"
  xdotool mousemove 5000 5000
  shot "$theme-fx"
  ui_step "Cancel (Escape)"                  ui_key Escape
  ui_mark
  ui_step "Appearance (search)"              ui_command "Appearance"
  ui_expect_new "Panel Appearance input value at" "the picker button's place"
  ui_step "open the picker"                  ui_click_logged "Panel Appearance input value"
  ui_expect_new "Appearance picker opened: " "the picker opened"
  ui_focus_dialog '^Appearance Picker$'
  shot "$theme-appearance"
  ui_step "close the picker (Escape)"        ui_key Escape
  ui_focus_main
  ui_step "Cancel (Escape)"                  ui_key Escape
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Material and Appearance tools look pictures in $DIR"
