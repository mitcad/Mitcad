#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/LibraryPicker.cpp app/browser/LibraryPicker.hpp app/commands/MaterialCommands.cpp app/print/MaterialMenuChoices.cpp app/print/MaterialMenuChoices.hpp app/framework/Appearances.cpp tools/cli
# The Appearance tool's picker lists the appearances given last under
# Recent (mitcad#1685), through the real UI, on a 40 x 30 x 20 mm box:
#   - At first the picker has no Recent.
#   - Paint - Red, then Filament - Black chosen in the picker and given to
#     Body1 with OK: the picker lists them first under Recent, after
#     Default, the latest first (Filament - Black, Paint - Red).
#   - The list is kept in the settings: after a restart the picker lists
#     them under Recent again, in the same order.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-appearance-recent-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-appearance-recent.XXXXXX)
FILE=$WORK/box.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -15],
    "length": 40, "width": 30, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# give name id: the Appearance tool on Body1, `name` chosen in its picker,
# OK.
give() {
  ui_mark
  ui_step "select Body1 in the browser"  ui_click_logged "Browser Root/Bodies/Body1"
  ui_step "Appearance (search)"          ui_command "Appearance"
  ui_expect_new "Appearance Bodies: 1 body [body F1.b0]" "the tool takes Body1"
  ui_step "the picker: $1"               ui_picker_choose "Panel Appearance input value" "Appearance picker" "$1"
  ui_mark
  ui_step "OK (Enter)"                   ui_key Return
  ui_expect_new "Appearance $2 for 1 body(ies)" "Body1 has $1"
  ui_step "clear the selection (Esc)"    ui_key Escape
}

# picker_recent expected sections: the Appearance tool on Body1, its
# picker opened; it lists `expected` under Recent ("none": no Recent) and
# its headings begin with `sections`; then closed and the tool cancelled.
picker_recent() {
  ui_mark
  ui_step "select Body1 in the browser"  ui_click_logged "Browser Root/Bodies/Body1"
  ui_step "Appearance (search)"          ui_command "Appearance"
  ui_expect_new "Panel Appearance input value at" "the panel is laid out"
  ui_mark
  ui_step "open the picker"              ui_click_logged "Panel Appearance input value"
  ui_expect_new "Appearance picker opened: " "the picker opened"
  ui_expect_new "Appearance picker recent: $1" "Recent: $1"
  ui_expect_new "Appearance picker sections: $2" "its headings begin with $2"
  ui_focus_dialog '^Appearance Picker$'
  ui_step "close the picker (Esc)"       ui_key Escape
  ui_expect_new "Appearance picker closed" "the picker closed"
  ui_focus_main
  ui_step "cancel the tool (Esc)"        ui_key Escape
  ui_step "clear the selection (Esc)"    ui_key Escape
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Opened $FILE" "the box opened"

echo "--- No Recent at first"
picker_recent "none" ""
# Without Recent the headings are the classes' and This design's only.
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Appearance picker sections: Recent"; then
  ui_fail "the picker has a Recent heading before any appearance was given"
fi
echo "ok   no Recent heading"

echo "--- Two appearances given"
give "Paint - Red" paint_red
ui_expect_new "Appearance tool: recent paint_red" "Paint - Red remembered"
give "Filament - Black" filament_black
ui_expect_new "Appearance tool: recent filament_black, paint_red" "Filament - Black remembered first"
picker_recent "Filament - Black | Paint - Red" "Recent 2 | "

echo "--- After a restart"
ui_stop_app
ui_start_app --open "$FILE"
ui_expect_log "Opened $FILE" "the box opened again"
picker_recent "Filament - Black | Paint - Red" "Recent 2 | "

ui_finish "UI Appearance picker's Recent"
