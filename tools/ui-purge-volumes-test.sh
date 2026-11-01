#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files app/browser/BrowserController.cpp app/framework/NoteRow.cpp
# check-all sources: app/framework/CommandPanel.cpp core/model core/3mf core/zip tools/cli
# Purge volumes on filament changes (mitcad#705) through the real UI, on a
# design of two boxes, one of a white filament and one of a black one:
#   1. 3D Print with 3MF: the Purge volumes row says Computed; its pencil
#      opens the table of the two filaments with the computed volumes
#      (white to black 140 mm³, black to white 420 mm³: dark to light
#      takes more).
#   2. A cell typed: the pair's own, marked as set by hand; the row says
#      1 set by hand.
#   3. Reset of the selected cell: the computed volume again.
#   4. The material editor's Filament tab (the Materials row's pencil): the
#      search finds the two purge fields; Purge into this filament typed,
#      and the table follows.
#   5. The other cell typed, Send to a (fake) Bambu Studio: the 3MF's
#      project settings (unzipped) carry the table, the filaments' values
#      and the multiplier; the design file keeps the pair; the edit was one
#      undo step.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-purge-volumes-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-purge-volumes.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/boxes.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
# The app's temporary folder, where 3D Print's files go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR"
FOLDER=$TMPDIR/mitcad-print/boxes
ARGS=$WORK/slicer-args.txt

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# expect_3mf filter expected description: a jq filter on the project
# settings of the 3MF sent.
expect_3mf() {
  local actual
  actual=$(unzip -p "$FOLDER/boxes.3mf" Metadata/project_settings.config | jq -c "$1") ||
    ui_fail "$3: cannot read the 3MF's project settings"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# Two 20 mm boxes: the first of a white PLA (the root's material), the
# second of a black PETG. Their colours give the purge values: white 210
# into and 70 out of it, black 70 into and 210 out of it.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "white-pla", "name": "White PLA", "based_on": "pla",
   "filament": {"filament_colour": "#FFFFFF"}},
  {"cmd": "create_material", "id": "black-petg", "name": "Black PETG", "based_on": "pla",
   "filament": {"filament_type": "PETG", "filament_colour": "#000000"}},
  {"cmd": "set_component_material", "component": "C0", "material": "white-pla"},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "black-petg"}
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
ui_expect_log "Body F2.b0 at" "the boxes are in the view"

echo "--- 1. 3D Print: the Purge volumes row and its table, computed"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer note purge: Computed [Edit]" "the row says the volumes are computed"
ui_expect_new "Panel Send to Slicer action purge.0 at" "its pencil has its place"
ui_mark
ui_step "the pencil"                       ui_click_logged "Panel Send to Slicer action purge.0"
ui_expect_new "Purge volumes opened: White PLA, Black PETG" "the table of the two filaments, in the order of use"
ui_expect_new "Purge volume White PLA → Black PETG = 140 mm³" "white to black: 70 out of white, 70 into black"
ui_expect_new "Purge volume Black PETG → White PLA = 420 mm³" "black to white: 210 and 210, dark to light takes more"
ui_expect_new "Purge volumes cell 0.1 at" "the cells have their places"
ui_focus_dialog '^Purge Volumes$'

echo "--- 2. A cell typed: set by hand, marked"
ui_mark
ui_step "White PLA → Black PETG"           ui_click_logged "Purge volumes cell 0.1"
ui_sync
xdotool type --delay 40 "300"
ui_key Return
ui_expect_new "Purge volume set: White PLA → Black PETG = 300 mm³" "Enter set it"
ui_expect_new "Purge volume White PLA → Black PETG = 300 mm³ (set by hand)" "the cell is marked as set by hand"
ui_expect_new "Panel Send to Slicer note purge: 1 set by hand [Edit]" "the row counts it"
summary=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Purge volume Black PETG → White PLA = " | tail -1)
case $summary in
  *"set by hand"*) ui_fail "the other pair changed: $summary" ;;
esac
echo "ok   the other pair stays computed"

echo "--- 3. Reset: computed again"
ui_focus_dialog '^Purge Volumes$'
ui_mark
ui_step "White PLA → Black PETG"           ui_click_logged "Purge volumes cell 0.1"
ui_step "Reset"                            ui_click_logged "Purge volumes reset"
ui_expect_new "Purge volumes reset: 1" "Reset took the pair's own volume away"
ui_expect_new "Purge volume White PLA → Black PETG = 140 mm³" "the computed volume again"
ui_expect_new "Panel Send to Slicer note purge: Computed [Edit]" "the row says computed again"

echo "--- 4. The material editor's Filament tab: the purge fields"
ui_focus_main
ui_mark
ui_step "the Materials row's pencil"       ui_click_logged "Panel Send to Slicer action materials.0"
ui_expect_new "Materials opened" "Materials opened for the bodies printed"
ui_expect_new "Materials item White PLA at" "White PLA is listed"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "White PLA"                        ui_click_logged "Materials item White PLA"
ui_expect_new "Material selected: White PLA (design)" "selected"
ui_expect_new "Materials settings tab filament at" "the print tabs beside General"
ui_mark
ui_step "the Filament tab"                 ui_click_logged "Materials settings tab filament"
ui_expect_new "Materials settings search at" "Search settings"
ui_mark
ui_step "search 'purge'"                   ui_click_logged "Materials settings search"
xdotool type --delay 40 "purge"
ui_sync
ui_expect_new "Materials settings field purge_into at" "Purge into this filament"
ui_expect_new "Materials settings field purge_out at" "Purge out of this filament"
ui_step "Purge into this filament"         ui_click_logged "Materials settings field purge_into"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "95"
ui_key Return
ui_expect_new "Material White PLA (design) purge_into = 95" "Enter saved it"
ui_expect_new "Purge volume Black PETG → White PLA = 305 mm³" "the table follows: 210 out of black, 95 into white"
ui_step "close"                            ui_click_logged "Materials close"

echo "--- 5. The other cell typed, Send: the table in the 3MF"
ui_focus_dialog '^Purge Volumes$'
ui_mark
ui_step "Black PETG → White PLA"           ui_click_logged "Purge volumes cell 1.0"
ui_sync
xdotool type --delay 40 "500"
ui_key Return
ui_expect_new "Purge volume Black PETG → White PLA = 500 mm³ (set by hand)" "set by hand"
ui_step "close"                            ui_click_logged "Purge volumes close"
ui_focus_main
rm -f "$ARGS"
ui_mark
ui_step "Send (Enter)"                     ui_key Return
ui_expect_new "3D Print: print settings for bambu_studio, filaments 1,2" "Bambu Studio's settings, two filaments"
for _ in $(seq 1 50); do
  [ -f "$ARGS" ] && break
  sleep 0.2
done
[ -f "$ARGS" ] || ui_fail "the slicer was not started"
[ "$(cat "$ARGS")" = "$FOLDER/boxes.3mf" ] || ui_fail "the slicer's arguments: $(cat "$ARGS")"
echo "ok   the slicer got the 3MF"
expect_3mf '.flush_volumes_matrix' '["0","140","500","0"]' \
  "the table: computed from white to black, set by hand from black to white"
expect_3mf '.flush_volumes_vector' '["70","95","210","70"]' "each filament's values, out and in"
expect_3mf '.flush_multiplier' '"1"' "the multiplier the slicer applies"
ui_focus_main
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[.purge_volumes[] | [.from, .to, (.volume + 0)]]' '[["black-petg","white-pla",500]]' \
  "the design keeps the pair"
expect_file '[.materials[] | select(.id == "white-pla") | (.filament.purge_into + 0)]' '[95]' \
  "and the material its value"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Set Purge Volume Black PETG to White PLA" "one undo step"
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Redo: Set Purge Volume Black PETG to White PLA" "redone"

ui_finish "UI purge volumes"
