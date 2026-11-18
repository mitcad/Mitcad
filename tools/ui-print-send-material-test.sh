#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files app/browser app/framework/NoteRow.cpp app/framework/CommandPanel.cpp app/commands/MaterialCommands.cpp
# check-all sources: core/model core/model-api core/3mf core/zip tools/cli
# A component's material reaches the slicer (mitcad#1052) through the real
# UI, on a design of two components, Lid and Box, a box each, and a PETG HF
# material of the design with its own nozzle and bed temperatures:
#   1. Material > More... of the Lid's row in the browser (the Material
#      tool, mitcad#1059) with the Lid picked, PETG HF chosen in its
#      picker, OK: the Lid's default material.
#   2. Send to Slicer, nothing selected: a row per body (mitcad#2008),
#      the Lid's material by its name, and the Box without one takes it
#      too: a print project cannot leave one body to the slicer, and the
#      list shows what is sent; the line under it says the project is
#      partly the slicer's defaults.
#   3. Send to a (fake) Bambu Studio: the log names each part's material
#      and own values; the 3MF's project settings (unzipped) give both
#      parts PETG HF's filament with its temperatures; the warning says
#      that no printer was chosen, in plain words (mitcad#1727).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-send-material-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-print-send-material.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/lid-box.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
# The app's temporary folder, where 3D Print's files go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR"
FOLDER=$TMPDIR/mitcad-print/lid-box
ARGS=$WORK/slicer-args.txt

# expect_3mf file filter expected description: a jq filter on the project
# settings of the 3MF sent.
expect_3mf() {
  local actual
  actual=$(unzip -p "$FOLDER/lid-box.3mf" Metadata/project_settings.config | jq -c "$1") ||
    ui_fail "$3: cannot read the 3MF's project settings"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# Lid at the origin and Box 40 mm away, a 20 mm box each; PETG HF.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Lid"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Box", "transform": [[1, 0, 0, 40], [0, 1, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_material", "id": "petg-hf", "name": "PETG HF", "based_on": "pla",
   "filament": {"filament_type": "PETG", "temperature": 255, "bed_temperature": 70}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# A fake Bambu Studio: writes the arguments it was started with; the 3MF
# format remembered.
cat > "$WORK/fake-slicer.sh" << EOF
#!/bin/sh
printf '%s\n' "\$@" > "$ARGS.part"
mv "$ARGS.part" "$ARGS"
EOF
chmod +x "$WORK/fake-slicer.sh"
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nslicerName=Bambu Studio (fake)\nslicerProgram=%s\nformat=3mf\n' "$WORK/fake-slicer.sh" \
  > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F2.b0 in O2 at" "the boxes are in the view"

echo "--- 1. Material > More... of the Lid: PETG HF given by the Material tool"
ui_mark
ui_step "right-click Lid:1"                ui_click_logged "Browser Root/Lid:1" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Material > More..."       ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for component C1" "the Material tool for the Lid component"
ui_expect_new "Panel Material bodies note: 1 component: Default" "the component is the tool's target"
ui_step "the picker: PETG HF"              ui_picker_choose "Panel Material input material" "Material picker" "PETG HF"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Default material of Lid: petg-hf" "the Lid's default material is PETG HF"

echo "--- 2. Send to Slicer names the Lid's material"
# The Lid's row stays selected, and 3D Print prints the selection: both
# components are sent with nothing selected.
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_command "Send to Slicer"
# A row per body (mitcad#2008): the Lid's material by its name, and the
# Box without a material of its own sent with it, as its row shows.
ui_expect_new "3D Print materials: Body1: PETG HF | Body2: PETG HF" \
  "the Lid's material by its name, the Box's row the same"
ui_expect_new "Panel Send to Slicer input material_1 shows: PETG HF" "the Lid's field"
ui_expect_new "Panel Send to Slicer input material_2 shows: PETG HF" "the Box's field"
ui_expect_new "3D Print delivery: project (partial)" "a print project, partly the slicer's defaults"

echo "--- 3. Send to Bambu Studio: the Lid's filament carries PETG HF's values"
rm -f "$ARGS"
ui_mark
ui_step "Send (Enter)"                     ui_key Return
ui_expect_new "3D Print delivery sent: project" "a print project"
ui_expect_new "3D Print: print settings for bambu_studio, filaments 1,1" "Bambu Studio's settings, one filament"
ui_expect_new "Print export: Body1 material petg-hf, " "the Lid's part is of PETG HF"
ui_expect_new "Print export: Body2 material petg-hf, " "the Box's as its row shows (mitcad#2008)"
# No printer chosen (mitcad#1727): the slicer gets a stand-in printer,
# which the notice says in plain words, naming no command.
ui_expect_new "no printer was chosen: the slicer gets a stand-in printer" \
  "the notice says the slicer cannot slice with the stand-in printer"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D Print warnings:" | grep -qF "import_print_profile"; then
  ui_fail "the shown warning names a command: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D Print warnings:" | tail -1)"
fi
echo "ok   the shown warning names no command"
for _ in $(seq 1 50); do
  [ -f "$ARGS" ] && break
  sleep 0.2
done
[ -f "$ARGS" ] || ui_fail "the slicer was not started"
[ "$(cat "$ARGS")" = "$FOLDER/lid-box.3mf" ] || ui_fail "the slicer's arguments: $(cat "$ARGS")"
echo "ok   the slicer got the 3MF"
expect_3mf '.filament_settings_id' '["PETG HF"]' "one filament, PETG HF"
expect_3mf '.filament_type' '["PETG"]' "its type"
expect_3mf '.nozzle_temperature' '["255"]' "PETG HF's nozzle temperature"
expect_3mf '.hot_plate_temp' '["70"]' "PETG HF's bed temperature"
model=$(unzip -p "$FOLDER/lid-box.3mf" Metadata/model_settings.config) || ui_fail "no model settings in the 3MF"
lid=$(printf '%s\n' "$model" | grep -A1 -F '<metadata key="name" value="Body1"/>' | tail -1)
case $lid in
  *'key="extruder" value="1"'*) echo "ok   the Lid's part prints with filament 1" ;;
  *) ui_fail "the Lid's part is not on filament 1: $model" ;;
esac

ui_finish "UI print send material"
