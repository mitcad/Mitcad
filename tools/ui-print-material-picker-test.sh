#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/browser/LibraryPicker.cpp app/browser/BrowserController.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/Command.hpp app/framework/NoteRow.cpp core/model tools/cli
# check-all sources: app/framework/PickerComboBox.cpp
# Send to Slicer's material rows (mitcad#1141) through the real UI, on a
# design of three boxes: Body1 without a material of its own, Body2 of Shop
# PETG and Body3 of Shop PLA, and Shop TPU, all three materials of the
# design's with a Bambu Studio filament preset:
#   1. Body1 selected, Send to Slicer: one row, Material, "No material:
#      choose one", not "Default (Steel)"; no warning names Steel.
#   2. Shop TPU chosen in the row's picker (the FDM plastics first, This
#      design's on top): Body1 is of Shop TPU at once, the row shows it, no
#      warning; the panel closed, one Undo takes it back.
#   3. Body2 and Body3 selected: two rows, Filament 1: Shop PETG (1 body)
#      and Filament 2: Shop PLA (1 body); Shop TPU chosen in Filament 1
#      changes Body2 only.
#   4. Edit materials…, the last row of Filament 2's picker, opens the
#      Materials window on Shop PLA.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-material-picker-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-print-material-picker.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/boxes.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library

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

# no_steel_warning: no warning since the mark names Steel (the bodies
# without a material are not warned about, mitcad#1141).
no_steel_warning() {
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "names no" | grep -qF "Steel"; then
    ui_fail "a warning names Steel: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "names no" | tail -1)"
  fi
  echo "ok   no warning names Steel"
}

# Three 20 mm boxes; three materials of the design's, each with a Bambu
# Studio filament preset.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [70, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "shop-tpu", "name": "Shop TPU", "based_on": "pla",
   "filament": {"filament_type": "TPU", "filament_settings_id": {"bambu_studio": "Generic TPU"}}},
  {"cmd": "create_material", "id": "shop-petg", "name": "Shop PETG", "based_on": "pla",
   "filament": {"filament_type": "PETG", "filament_settings_id": {"bambu_studio": "Generic PETG"}}},
  {"cmd": "create_material", "id": "shop-pla", "name": "Shop PLA", "based_on": "pla",
   "filament": {"filament_type": "PLA", "filament_settings_id": {"bambu_studio": "Generic PLA"}}},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "shop-petg"},
  {"cmd": "set_body_material", "uid": "F3.b0", "material": "shop-pla"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# 3MF, whose print rows are enabled (the design's slicer, Bambu Studio).
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nformat=3mf\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F3.b0 at" "the boxes are in the view"
ui_expect_log "Browser Root/Bodies/Body3 at" "the browser lists the bodies"

echo "--- 1. Body1 without a material: \"No material: choose one\""
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Browser selected: Root/Bodies/Body1" "Body1 selected"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "3D Print materials: Material: No material: choose one" "one row, Body1 without a material"
ui_expect_new "Panel Send to Slicer input material_1 shows: No material: choose one" \
  "the field says so, not Default (Steel)"
ui_expect_new "Panel Send to Slicer input material_1 at" "the Material row has its place"
no_steel_warning

echo "--- 2. Shop TPU chosen in the row: Body1's material at once, one undo step"
ui_mark
ui_step "open the Material row"            ui_click_logged "Panel Send to Slicer input material_1"
ui_expect_new "Material picker opened: " "its picker opened"
ui_expect_new "Material picker sections: This design 3 |" "the design's three filaments on top"
ui_focus_dialog '^Material Picker$'
xdotool type --delay 40 -- "Shop TPU"
ui_expect_new "Material picker search: Shop TPU, first Shop TPU" "Shop TPU found"
ui_step "choose (Enter)"                   ui_key Return
ui_expect_new "Material picker chose: Shop TPU" "Shop TPU chosen"
ui_focus_main
ui_expect_new "3D Print material: Shop TPU for Body1" "given to Body1 at once"
ui_expect_new "Material shop-tpu set for 1 body(ies), appearance replace" "as the Material tool logs it"
ui_expect_new "3D Print materials: Material: Shop TPU" "the row shows Shop TPU"
ui_expect_new "Panel Send to Slicer input material_1 shows: Shop TPU" "the field shows Shop TPU"
no_steel_warning
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed; the material stays"
save
expect_file '.bodies[0].material' '"shop-tpu"' "Body1 is of Shop TPU"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Material of Body1" "one undo step"
save
expect_file '.bodies[0].material' 'null' "Body1 has no material of its own again"

echo "--- 3. Body2 (Shop PETG) and Body3 (Shop PLA): two rows, one changed alone"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "click Body2"                      ui_click_logged "Browser Root/Bodies/Body2"
ui_expect_new "Browser selected: Root/Bodies/Body2" "Body2 selected"
ui_step "Ctrl+click Body3"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body3") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body2, Root/Bodies/Body3" "Body2 and Body3 selected"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "3D Print materials: Filament 1: Shop PETG (1 body) | Filament 2: Shop PLA (1 body)" \
  "a row per filament"
ui_expect_new "Panel Send to Slicer label material_1: Filament 1" "the first row is Filament 1"
ui_expect_new "Panel Send to Slicer label material_2: Filament 2" "the second Filament 2"
ui_expect_new "Panel Send to Slicer input material_1 at" "Filament 1 has its place"
ui_mark
ui_step "open Filament 1"                  ui_click_logged "Panel Send to Slicer input material_1"
ui_expect_new "Material picker opened: " "its picker opened"
ui_focus_dialog '^Material Picker$'
xdotool type --delay 40 -- "Shop TPU"
ui_expect_new "Material picker search: Shop TPU, first Shop TPU" "Shop TPU found"
ui_step "choose (Enter)"                   ui_key Return
ui_expect_new "Material picker chose: Shop TPU" "Shop TPU chosen"
ui_focus_main
ui_expect_new "3D Print material: Shop TPU for Body2" "given to Body2 only"
ui_expect_new "3D Print materials: Filament 1: Shop TPU (1 body) | Filament 2: Shop PLA (1 body)" \
  "Filament 2 stays Shop PLA"

echo "--- 4. Edit materials… of Filament 2: the Materials window on Shop PLA"
ui_expect_log "Panel Send to Slicer input material_2 at" "Filament 2 has its place"
ui_mark
ui_step "open Filament 2"                  ui_click_logged "Panel Send to Slicer input material_2"
ui_expect_new "Material picker opened: " "its picker opened"
ui_focus_dialog '^Material Picker$'
xdotool type --delay 40 -- "Edit materials"
ui_expect_new "Material picker search: Edit materials, first Edit materials…" "its last row, Edit materials…"
ui_step "Edit materials… (Enter)"          ui_key Return
ui_expect_new "Material picker chose: Edit materials…" "Edit materials… chosen"
ui_expect_new "3D Print: Edit materials… of Shop PLA (Body3)" "for Filament 2's material"
ui_expect_new "Materials opened for bodies F3.b0" "Materials opened for Body3"
ui_expect_new "Material selected: Shop PLA (design)" "on Shop PLA"
ui_focus_dialog '^Materials$'
ui_step "close"                            ui_click_logged "Materials close"
ui_focus_main
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"
save
expect_file '[.bodies[1].material, .bodies[2].material]' '["shop-tpu","shop-pla"]' \
  "Body2 is of Shop TPU, Body3 still of Shop PLA"

ui_finish "UI print material picker"
