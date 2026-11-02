#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/framework/NoteRow.cpp app/browser/LibraryPicker.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/Command.hpp core/model core/3mf core/zip tools/cli
# A material's filament preset per slicer (mitcad#1062) through the real
# UI, on a box whose material (Shop PETG HF, made in MitCad) names no
# slicer preset:
#   1. 3D Print with 3MF, to a slicer of no known dialect (the design's
#      slicer, Bambu Studio, then): the warning under the Material row says
#      that Shop PETG HF names no Bambu Studio filament preset.
#   2. Edit materials…, the last row of the Material row's picker
#      (mitcad#1141), opens Materials on the body's material; on the
#      Filament tab the search finds the Slicer presets, a row per slicer;
#      Bambu Studio's typed and Enter: one edit, the name kept for Bambu
#      Studio. Closed, the warning is gone.
#   3. Send: the 3MF's project settings name the filament by the preset
#      (`filament_settings_id`) and inherit from it (`inherits_group`).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-filament-preset-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-filament-preset.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/box.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
# The app's temporary folder, where 3D Print's files go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR"
FOLDER=$TMPDIR/mitcad-print/box
ARGS=$WORK/slicer-args.txt
WARNING="(warning: Shop PETG HF names no Bambu Studio filament preset"

# expect_3mf filter expected description: a jq filter on the project
# settings of the 3MF sent.
expect_3mf() {
  local actual
  actual=$(unzip -p "$FOLDER/box.3mf" Metadata/project_settings.config | jq -c "$1") ||
    ui_fail "$3: cannot read the 3MF's project settings"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# A 20 mm box of Shop PETG HF, a material of the design's own that names
# no slicer preset.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "shop-petg-hf", "name": "Shop PETG HF", "based_on": "pla",
   "filament": {"filament_type": "PETG", "temperature": 250}},
  {"cmd": "set_body_material", "uid": "F1.b0", "material": "shop-petg-hf"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# A fake slicer of no known dialect: writes the arguments it was started
# with; the 3MF format remembered.
cat > "$WORK/fake-slicer.sh" << EOF
#!/bin/sh
printf '%s\n' "\$@" > "$ARGS.part"
mv "$ARGS.part" "$ARGS"
EOF
chmod +x "$WORK/fake-slicer.sh"
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nslicerName=Fake slicer\nslicerProgram=%s\nformat=3mf\n' "$WORK/fake-slicer.sh" \
  > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the box is in the view"

echo "--- 1. 3D Print: a warning under the Material row of the material without a preset"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "$WARNING" "the warning that Shop PETG HF names no Bambu Studio preset"
ui_expect_new "3D Print materials warning: Shop PETG HF names no Bambu Studio filament preset" "the warning logged"
ui_expect_new "Panel Send to Slicer input material_1 at" "the Material row has its place"

echo "--- 2. Edit materials…: Materials' Filament tab, Bambu Studio's preset typed"
ui_mark
ui_step "open the Material row"            ui_click_logged "Panel Send to Slicer input material_1"
ui_expect_new "Material picker opened: " "its picker opened"
ui_focus_dialog '^Material Picker$'
xdotool type --delay 40 -- "Edit materials"
ui_expect_new "Material picker search: Edit materials, first Edit materials…" "its last row, Edit materials…"
ui_step "Edit materials… (Enter)"          ui_key Return
ui_expect_new "Material picker chose: Edit materials…" "Edit materials… chosen"
ui_expect_new "Materials opened for bodies F1.b0" "Materials opened for the body printed"
ui_expect_new "Material selected: Shop PETG HF (design)" "on the body's material"
ui_expect_new "Materials settings tab filament at" "the print tabs beside General"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "the Filament tab"                 ui_click_logged "Materials settings tab filament"
ui_expect_new "Materials settings search at" "Search settings"
ui_mark
ui_step "search 'preset'"                  ui_click_logged "Materials settings search"
xdotool type --delay 40 "preset"
ui_sync
ui_expect_new "Materials settings field filament_settings_id.bambu_studio at" "Bambu Studio's preset row"
ui_expect_new "Materials settings field filament_settings_id.prusaslicer at" "PrusaSlicer's, a row per slicer"
ui_step "Bambu Studio's preset"            ui_click_logged "Materials settings field filament_settings_id.bambu_studio"
ui_sync
ui_mark
ui_key ctrl+a
xdotool type --delay 40 "Generic PETG HF"
ui_key Return
ui_expect_new "Material Shop PETG HF (design) filament_settings_id = Bambu Studio: Generic PETG HF" \
  "Enter saved it, for Bambu Studio"
ui_step "close"                            ui_click_logged "Materials close"
ui_focus_main
ui_expect_new "3D Print materials warning: none" "the warning is gone"

echo "--- 3. Send: the 3MF names the filament by the preset and inherits from it"
rm -f "$ARGS"
ui_mark
ui_step "Send (Enter)"                     ui_key Return
ui_expect_new "3D Print: print settings for bambu_studio, filaments 1" "Bambu Studio's settings, one filament"
for _ in $(seq 1 50); do
  [ -f "$ARGS" ] && break
  sleep 0.2
done
[ -f "$ARGS" ] || ui_fail "the slicer was not started"
[ "$(cat "$ARGS")" = "$FOLDER/box.3mf" ] || ui_fail "the slicer's arguments: $(cat "$ARGS")"
echo "ok   the slicer got the 3MF"
expect_3mf '.filament_settings_id' '["Generic PETG HF"]' "the filament is named by the preset"
expect_3mf '.inherits_group' '["","Generic PETG HF",""]' "the filament inherits from it"

ui_finish "UI filament preset"
