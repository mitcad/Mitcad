#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/Appearances.cpp app/framework/Appearances.hpp app/commands/MaterialCommands.cpp app/files/MainWindowPrint.cpp
# Edit Appearances (mitcad#46) through the real UI, on a 40 x 30 x 20 mm box:
#   - The dialog lists the library with swatches, and shows a library
#     appearance read-only; it assigns nothing (mitcad#1059).
#   - New copies Chrome into the design; its name, roughness and base
#     colour are edited in the dialog, each an undo step.
#   - The Appearance tool gives the selected body the appearance, chosen in
#     its picker: the shaded view shows it in its base colour, the project
#     file keeps the appearance and the body's id.
#   - Edit Appearances again: Delete removes the appearance and the body
#     gets the default look; undo takes the steps back.
#   - Edit Appearances… last in the Appearance tool's picker (mitcad#1415):
#     the window opens on the tool's appearance (Red Chrome) while the
#     panel stays open; New there, renamed Blue Chrome; once the window
#     closes the picker offers it, and OK gives it to Body1, still picked,
#     in one undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-appearance-test.sh [screenshot of the dialog.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-appearance.XXXXXX)
FILE=$WORK/box.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_file expression expected description: a Python expression on the
# saved project file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# dialog_type "field" text: replaces a field's text in the dialog, Enter.
dialog_type() {
  ui_focus_dialog '^Appearances$'
  ui_click_logged "Appearances field $1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 -- "$2"
  ui_key Return
}

# picker_edit button picker entry: the picker opened from its button (after
# a ui_mark), its last entry (mitcad#1415) found by typing Edit and chosen
# with Return, as the keyboard reaches the other entries.
picker_edit() {
  ui_click_logged "$1"
  ui_expect_new "$2 opened: " "the $2 opened"
  ui_focus_dialog "^${2% picker} Picker\$"
  xdotool type --delay 40 -- "Edit"
  ui_expect_new "$2 search: Edit, first $3" "typing Edit finds $3"
  ui_key Return
  ui_expect_new "$2 chose: $3" "$3 chosen in the $2"
}

# warm_pixels: red pixels in the middle of the view (the box).
warm_pixels() {
  ui_sync
  xwd -root -silent > "$WORK/shot.xwd" || ui_fail "no screenshot"
  local x0 y0 x1 y1
  read -r x0 y0 <<< "$(ui_view_at 30 30)"
  read -r x1 y1 <<< "$(ui_view_at 70 70)"
  python3 "$STATS" "$WORK/shot.xwd" "$x0" "$y0" $((x1 - x0)) $((y1 - y0)) |
    python3 -c 'import json, sys; print(json.load(sys.stdin)["warm"])'
}

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -15],
    "length": 40, "width": 30, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Opened $FILE" "the box opened"
before=$(warm_pixels)

echo "--- The dialog and the library"
ui_mark
ui_step "select Body1 in the browser"   ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Edit Appearances (search)"     ui_command "Edit Appearances"
ui_expect_new "Appearances dialog opened" "the dialog opened"
ui_expect_new "Appearances item steel_satin at" "the library is listed"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "click Steel - Satin"           ui_click_logged "Appearances item steel_satin"
ui_step "type Chrome (list search)"     xdotool type --delay 60 "Chrome"
ui_expect_new "Appearance selected: chrome" "Chrome selected"

echo "--- A copy of Chrome in the design"
ui_mark
ui_step "New"                           ui_click_logged "Appearances new"
ui_expect_new "Created appearance custom1 (Chrome Copy) from chrome" "a copy of Chrome"
ui_expect_new "Appearances item custom1 at" "listed under the design's appearances"
ui_step "name it Red Chrome, Enter"     xdotool type --delay 40 "Red Chrome"
ui_key Return
ui_expect_new "Appearance custom1 name = Red Chrome" "renamed"
ui_mark
ui_step "roughness 0.35"                dialog_type roughness 0.35
ui_expect_new "Appearance custom1 roughness = 0.35" "roughness changed"
ui_step "base colour #d02020"           dialog_type base_color "#d02020"
ui_expect_new "Appearance custom1 base_color = #d02020" "base colour changed"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- The Appearance tool assigns it"
ui_step "Close"                         ui_click_logged "Appearances close"
ui_focus_main
ui_mark
ui_step "Appearance (search)"           ui_command "Appearance"
ui_expect_new "Appearance Bodies: 1 body [body F1.b0]" "the tool takes the selected Body1"
ui_step "the picker: Red Chrome"        ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red Chrome"
ui_mark
ui_step "OK (Enter)"                    ui_key Return
ui_expect_new "Appearance custom1 for 1 body(ies)" "assigned"
ui_step "clear the selection (Esc)"     ui_key Escape
after=$(warm_pixels)
[ "$after" -gt $((before + 1000)) ] || ui_fail "the box is not shown red ($before -> $after red pixels)"
echo "ok   the box is shown in its base colour ($before -> $after red pixels)"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[(a["id"], a["name"], a["roughness"], a["metalness"]) for a in doc["appearances"]]' \
  "[('custom1', 'Red Chrome', 0.35, 1.0)]" "the file keeps the appearance"
expect_file '[round(c * 255) for c in doc["appearances"][0]["base_color"]]' "[208, 32, 32]" \
  "with its base colour"
expect_file 'doc["bodies"][0]["appearance"]' "custom1" "and the body's appearance"

echo "--- Edit Appearances, Delete and undo"
ui_mark
ui_step "Edit Appearances (search)"     ui_command "Edit Appearances"
ui_expect_new "Appearances dialog opened" "the dialog opened again"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "Delete"                        ui_click_logged "Appearances delete"
ui_expect_new "Deleted appearance custom1" "deleted"
ui_step "Close"                         ui_click_logged "Appearances close"
ui_focus_main
gone=$(warm_pixels)
[ "$gone" -lt $((after - 1000)) ] || ui_fail "the box is still red ($after -> $gone red pixels)"
echo "ok   the box has the default look again"
for step in "Delete Appearance Red Chrome" "Set Appearance of Body1" "Edit Appearance Red Chrome"; do
  ui_mark
  ui_step "undo"                        ui_key ctrl+z
  ui_expect_new "Undo: $step" "undo: $step"
done
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[(a["id"], a["name"]) for a in doc["appearances"]]' "[('custom1', 'Red Chrome')]" \
  "the appearance is back"
expect_file 'doc["bodies"][0].get("appearance")' "None" "unassigned"

echo "--- Edit Appearances… at the end of the tool's picker (mitcad#1415)"
ui_mark
ui_step "select Body1 in the browser"   ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Appearance (search)"           ui_command "Appearance"
ui_expect_new "Appearance Bodies: 1 body [body F1.b0]" "the tool takes the selected Body1"
ui_step "the picker: Red Chrome"        ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red Chrome"
ui_mark
ui_step "the picker: Edit Appearances…" picker_edit "Panel Appearance input value" "Appearance picker" "Edit Appearances…"
ui_expect_new "Appearance tool: Edit Appearances… on Red Chrome" "the tool opens the window on its appearance"
ui_expect_new "Appearances dialog opened" "the window opened"
ui_expect_new "Appearances on custom1" "on Red Chrome"
ui_focus_dialog '^Appearances$'
ui_mark
ui_step "New"                           ui_click_logged "Appearances new"
ui_expect_new " (Red Chrome Copy) from custom1" "a copy of Red Chrome while the tool's panel is open"
made=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  sed -n 's/.*Created appearance \([^ ]*\) (Red Chrome Copy) from custom1.*/\1/p' | tail -1)
[ -n "$made" ] || ui_fail "no id of the new appearance"
ui_step "name it Blue Chrome, Enter"    xdotool type --delay 40 "Blue Chrome"
ui_key Return
ui_expect_new "Appearance $made name = Blue Chrome" "renamed"
ui_mark
ui_step "Close"                         ui_click_logged "Appearances close"
ui_expect_new "Appearances closed" "closed"
ui_expect_new "Appearance tool: appearances read again, " "the tool read its appearances again"
ui_expect_new ", new: Blue Chrome" "Blue Chrome is offered"
ui_focus_main
ui_step "the picker: Blue Chrome"       ui_picker_choose "Panel Appearance input value" "Appearance picker" "Blue Chrome"
ui_expect_new "Panel Appearance tile value: Blue Chrome (This design)" "the picker button shows Blue Chrome"
ui_mark
ui_step "OK (Enter)"                    ui_key Return
ui_expect_new "Appearance $made for 1 body(ies)" "Body1, still picked, gets Blue Chrome"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file 'doc["bodies"][0].get("appearance")' "$made" "the body has the new appearance"
ui_mark
ui_step "undo"                          ui_key ctrl+z
ui_expect_new "Undo: Set Appearance of Body1" "one undo step"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file 'doc["bodies"][0].get("appearance")' "None" "unassigned again"
expect_file '[a["name"] for a in doc["appearances"]]' "['Red Chrome', 'Blue Chrome']" \
  "the new appearance stays in the design"

ui_finish "Edit Appearances"
