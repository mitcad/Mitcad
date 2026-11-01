#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/framework/NoteRow.cpp app/framework/CommandPanel.cpp app/browser app/view app/files
# check-all sources: app/OcctViewer.cpp app/MainWindowContextMenu.cpp core/model core/3mf core/vcs tools/cli
# Print settings and materials (mitcad#640, mitcad#654, mitcad#655) through
# the real UI, on a design of two boxes:
#   1. 3D Print with 3MF: with no process chosen (Slicer default) Own
#      settings names Body2's own walls (mitcad#747); Import from slicer…
#      of a slicer's project (a 3MF
#      in OrcaSlicer's form, written by mitcad-cli): the Printer and Process
#      rows show the imported ones, the line at the panel's end says where
#      the materials went, Details… shows the whole report; Cancel keeps
#      the import, one undo step.
#   2. Edit of the Process row: Printers and Processes on the design's
#      process; the search finds Sparse infill density by its plain name,
#      250 % is refused at the field, 25 typed and Enter is one undo step;
#      Save to My library puts it there. New › Process makes one in this
#      design as its process at once; Delete asks first, naming what uses
#      it and the outcome; Use in this design takes Standard again. Another
#      design changes Standard in My library: Printers and Processes asks
#      once, and Keep this design's settings keeps the design's values.
#      Its Import from Slicer… (mitcad#829) brings a Bambu Studio project's
#      printer and process into the design, selected, with Details…; one
#      undo takes the import back.
#      3D Print's New process… twice makes Process 1 and Process 2,
#      listed above a separator and the action; Up from the action skips
#      the separator (mitcad#828); undone again.
#   3. Edit Materials: Duplicate of PLA makes the user's own copy in this
#      design (Save to offered), its name and its nozzle temperature
#      typed; the Material tool (Material ▸ More… of Body1) assigns it.
#   4. Property Sets of a body: the effective print settings by category
#      with their source, a summary; the pencil of Own settings opens
#      Body1's own settings in the settings form, where Walls typed is the
#      body's own (Source: Body1) and its ↺ clears it. On the design's
#      Property Sets a right click on Process's ↺ runs nothing, and the
#      next left click on Material opens Materials on the material, to
#      edit it (mitcad#761; assigning is the Material tool's, mitcad#1059). In a
#      narrower window the pencils of Printer, Process, Material and Own
#      settings lie inside the Value column, after the names as shown;
#      Material's pencil opens Materials, Process's Printers and Processes
#      and no drop-down; Process's drop-down closed with Escape ends its
#      editor, and a second process made there (New › Process, undone after)
#      leaves Standard to choose: it is set and ends the editor (mitcad#827).
#   5. Show Build Volume, and 3D Print to a (fake) QIDI Studio sends a 3MF
#      with QIDI Studio's print settings.
#   6. A Bambu Studio project whose two filaments are slowed for cooling on
#      overhangs from 95% (mitcad#654) imported: nothing left unread.
#   7. 3D Print chosen and sent by Enter at once, with the exports made
#      slow (mitcad#888): the panel's question about changed library
#      profiles, due a moment after it opens, waits for the export's job
#      instead of reading the document during it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-settings-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-print-settings.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/boxes.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
# The app's temporary folder, where 3D Print's files go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR"
ARGS=$WORK/slicer-args.txt

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# value_column: the Value column of Property Sets as last logged, "from to"
# in the main window's coordinates (mitcad#827).
value_column() {
  grep -F "Property sets value column from " "$UI_LOG" | tail -1 |
    sed -n 's/.* from \(-\{0,1\}[0-9]*\) to \(-\{0,1\}[0-9]*\)$/\1 \2/p'
}

# expect_in_value_column "logged name" description: the glyph last logged
# under that name lies whole inside the Value column (mitcad#827).
expect_in_value_column() {
  local from to px
  read -r from to <<< "$(value_column)"
  px=$(grep -F "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at \(-\{0,1\}[0-9]*\),.*/\1/p')
  [ -n "$from" ] && [ -n "$to" ] && [ -n "$px" ] || ui_fail "$2: no place of '$1' or of the Value column logged"
  [ $((px - 8)) -ge "$from" ] && [ $((px + 8)) -le "$to" ] ||
    ui_fail "$2: '$1' at x $px lies outside the Value column ($from to $to)"
  echo "ok   $2"
}

# Ctrl+S is the main window's shortcut: not one of a print window's.
save() {
  ui_focus_main
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# The slicer's project: a box of PLA with a 256 mm bed printer and a
# standard process, exported for OrcaSlicer.
cat > "$WORK/slicer.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "printer", "name": "Bench X 0.4 nozzle",
   "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]], "max_print_height": 250,
              "nozzle_diameter": 0.4, "printer_settings_id": {"orcaslicer": "Bench X 0.4 nozzle"}}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Standard",
   "values": {"layer_height": 0.2, "perimeters": 3, "fill_density": 15}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "bench-x-0-4-nozzle"},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "standard"},
  {"cmd": "set_component_material", "component": "C0", "material": "pla"},
  {"cmd": "export", "path": "$WORK/plate.3mf", "dialect": "orcaslicer"}
]
EOF
"$CLI" run "$WORK/slicer.json" --save "$WORK/slicer.mitcad" > "$WORK/cli-slicer.log" 2>&1 ||
  { cat "$WORK/cli-slicer.log"; ui_fail "mitcad-cli (the slicer's project)"; }
[ -s "$WORK/plate.3mf" ] || ui_fail "the slicer's project was not written"

# A Bambu Studio project (mitcad#654): two boxes of two materials whose
# filaments are slowed for cooling on overhangs from 95%.
cat > "$WORK/mount.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "printer", "name": "Bench Mini 0.4 nozzle",
   "values": {"bed_shape": [[0, 0], [180, 0], [180, 180], [0, 180]], "max_print_height": 180,
              "nozzle_diameter": 0.4, "printer_settings_id": {"bambu_studio": "Bench Mini 0.4 nozzle"}}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Extra Fine", "values": {"layer_height": 0.08}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "bench-mini-0-4-nozzle"},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "extra-fine"},
  {"cmd": "create_material", "id": "mount-pla", "name": "Mount PLA", "based_on": "pla",
   "filament": {"overhang_threshold_participating_cooling": "95"}},
  {"cmd": "create_material", "id": "mount-abs", "name": "Mount ABS", "based_on": "abs",
   "filament": {"overhang_threshold_participating_cooling": "95"}},
  {"cmd": "set_component_material", "component": "C0", "material": "mount-pla"},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "mount-abs"},
  {"cmd": "export", "path": "$WORK/mount.3mf", "dialect": "bambu_studio"}
]
EOF
"$CLI" run "$WORK/mount.json" --save "$WORK/mount.mitcad" > "$WORK/cli-mount.log" 2>&1 ||
  { cat "$WORK/cli-mount.log"; ui_fail "mitcad-cli (the Bambu Studio project)"; }
[ -s "$WORK/mount.3mf" ] || ui_fail "the Bambu Studio project was not written"

# The design: a 20 mm box at the origin and one 300 mm away, the far one
# (Body2) with walls of its own and no printer or process yet (mitcad#747).
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [300, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "set_property", "owner": "F2.b0", "set": "Print Process", "key": "perimeters", "value": 4}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# A fake QIDI Studio: writes the arguments it was started with; the 3MF
# format remembered.
cat > "$WORK/fake-slicer.sh" << EOF
#!/bin/sh
printf '%s\n' "\$@" > "$ARGS.part"
mv "$ARGS.part" "$ARGS"
EOF
chmod +x "$WORK/fake-slicer.sh"
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nslicerName=QIDI Studio (fake)\nslicerProgram=%s\nformat=3mf\n' "$WORK/fake-slicer.sh" \
  > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F2.b0 at" "the boxes are in the view"

echo "--- 1. 3D Print: Import from slicer…, its line and Details…"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "3D Print printer: Slicer default" "the design has no printer yet"
ui_expect_new "Panel Send to Slicer note process_overrides: Body2: Walls [Edit]" \
  "Own settings names Body2's own walls with the process at Slicer default (mitcad#747)"
ui_expect_new "Panel Send to Slicer action import.0 at" "Import from slicer… is shown with 3MF"
ui_step "Import from slicer…"              ui_click_logged "Panel Send to Slicer action import.0"
ui_type_path "Import from Slicer" "$WORK/plate.3mf"
ui_focus_main
ui_expect_new "Imported print settings from plate.3mf: printers: Bench X 0.4 nozzle; processes: Standard" \
  "the printer and the process came in"
ui_expect_new "3D Print import: plate.3mf imported" \
  "the line names the file, nothing failed"
ui_expect_new "[Import from slicer…] [plate.3mf imported]" \
  "one button, the line under it the link to Details"
ui_expect_new "[Assign…]" "the Materials row offers the new material to the bodies"
ui_expect_new "3D Print printer: design/bench-x-0-4-nozzle" "the Printer row shows the imported printer"
ui_expect_new "3D Print process: design/standard" "and the Process row the process"
ui_expect_new "Panel Send to Slicer action import.1 at" "Details… has its place"
ui_mark
ui_step "Details…"                         ui_click_logged "Panel Send to Slicer action import.1"
ui_expect_new "Import details opened: " "the whole report opened"
# The export names the family's application (BambuStudio-…) for every
# member, so the import reads the file as Bambu Studio's project; a project
# OrcaSlicer itself wrote names OrcaSlicer (core print_files tests).
ui_expect_new "Import details: Imported plate.3mf (Bambu Studio) into this design." "its first line"
ui_expect_new "Import details: Printer: Bench X 0.4 nozzle" "the printer by name"
ui_focus_dialog '^Import Details$'
ui_step "close the report (Esc)"           ui_key Escape
ui_expect_new "Import details closed" "the report closed"
ui_focus_main
ui_mark
ui_step "Cancel the panel (Esc)"           ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"
save
expect_file '[.print_profiles[] | .id]' '["bench-x-0-4-nozzle","standard"]' "Cancel kept the import"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Import Print Settings plate.3mf" "one undo step"
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Redo: Import Print Settings plate.3mf" "redone"

echo "--- 2. Printers and Processes: Edit of the Process row"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer button_process at" "Edit beside the Process row"
ui_step "Edit"                             ui_click_logged "Panel Send to Slicer button_process"
ui_expect_new "Printers and processes opened" "the window opened"
ui_expect_new "Profile selected: process Standard (design)" "on the design's process"
ui_expect_new "Profile Standard value fill_density = 15 %" "the infill density in its own unit"
ui_expect_new "Profiles duplicate at" "its buttons are under the list"
ui_expect_new "Profiles settings search at" "its search has its place"
ui_focus_dialog '^Printers and Processes$'
ui_mark
ui_step "search 'infill density'"          ui_click_logged "Profiles settings search"
xdotool type --delay 40 "infill density"
ui_sync
ui_expect_new "Profiles settings field fill_density at" "the search finds the field"
ui_step "the field"                        ui_click_logged "Profiles settings field fill_density"
ui_key ctrl+a
xdotool type --delay 40 "250"
ui_key Return
ui_expect_new "Setting fill_density refused: " "250 % is refused at the field"
summary=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Profile Standard (design) fill_density = 250" | tail -1)
[ -z "$summary" ] || ui_fail "250 % was taken: $summary"
echo "ok   and not saved"
ui_key ctrl+a
xdotool type --delay 40 "25"
ui_key Return
ui_expect_new "Profile Standard (design) fill_density = 25 %" "Enter saved it"
ui_expect_new "Profile Standard value fill_density = 25 %" "the window shows the model's value"
ui_mark
ui_step "Save to My library"               ui_click_logged "Profiles save_to"
ui_expect_new "Profile saved to user: Standard" "saved to My library"
ui_expect_new "Profile shown: process Standard (user)" "the window follows it there"

echo "--- 2b. Printers and Processes: New process, the design's at once; Delete asked"
ui_mark
ui_step "New"                              ui_click_logged "Profiles new"
ui_expect_new "Profiles new menu: Printer | Process" "a printer or a process"
ui_key Down Down Return
ui_expect_new "Profile created: process " "a new process in this design"
ui_expect_new "Profile shown: process " "the window shows it"
created=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Profile shown: process " | tail -1 | sed 's/.*Profile shown: process \(.*\) (design).*/\1/')
[ -n "$created" ] || ui_fail "no name of the new process"
ui_expect_new "$created (design), the design's" "made the design's process in the same step"
ui_expect_new "Profiles save_to at" "Save to for the user's own new process"
ui_expect_new "Profiles delete at" "Delete has its place"
ui_mark
ui_step "Delete"                           ui_click_logged "Profiles delete"
ui_expect_new "Profiles delete: Delete $created from this design? This design then uses Slicer default." \
  "Delete asks first and says what follows"
ui_focus_dialog '^Delete$'
ui_step "Delete (Alt+D)"                   ui_key alt+d
ui_expect_new "Profile deleted: $created (design)" "deleted"
ui_focus_dialog '^Printers and Processes$'
ui_expect_new "Profiles item Standard at" "Standard is listed"
ui_step "Standard"                         ui_click_logged "Profiles item Standard"
ui_expect_new "Profile selected: process Standard (user)" "Standard selected"
ui_mark
ui_step "Use in this design"               ui_click_logged "Profiles use"
ui_expect_new "Profile used in this design: process Standard" "the design's process again"
ui_step "close"                            ui_click_logged "Profiles close"
ui_focus_main
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: " "the choice is one undo step"
ui_step "redo (Ctrl+Y)"                    ui_key ctrl+y
ui_expect_new "Redo: " "redone"

echo "--- 2c. Changed in another design: asked once, Keep this design's settings"
# Another design changes Standard in My library: its next version, another
# layer height.
cat > "$WORK/other.json" << 'EOF'
[
  {"cmd": "edit_print_profile", "library": "user", "id": "standard", "values": {"layer_height": 0.28}}
]
EOF
"$CLI" run "$WORK/other.json" --save "$WORK/other.mitcad" > "$WORK/cli-other.log" 2>&1 ||
  { cat "$WORK/cli-other.log"; ui_fail "mitcad-cli (another design's change)"; }
ui_mark
ui_command "Edit Printers and Processes"
ui_expect_new "Profile selected: process Standard (user)" "on the design's process"
ui_expect_new "Profiles changed elsewhere: Standard" "asked about the other design's change"
ui_focus_dialog '^Changed in Another Design$'
ui_step "Keep (Esc)"                        ui_key Escape
ui_expect_new "Profiles changed elsewhere: keep" "the design keeps its values"
ui_expect_new "Profile Standard: decline_print_library_update" "kept, one undo step"
ui_expect_new "Profile Standard value layer_height = 0.2 mm" "the design's layer height stays"
ui_focus_dialog '^Printers and Processes$'

echo "--- 2d. Printers and Processes: Import from Slicer… into the design, one undo step (mitcad#829)"
ui_expect_log "Profiles import at" "Import from Slicer… has its place"
ui_mark
ui_step "Import from Slicer…"              ui_click_logged "Profiles import"
ui_type_path "Import from Slicer" "$WORK/mount.3mf"
ui_expect_new "Imported print settings from mount.3mf: printers: Bench Mini 0.4 nozzle; processes: Extra Fine" \
  "the project's printer and process came into the design"
ui_expect_new "Profiles import: mount.3mf imported" "the line names the file"
ui_expect_new "Profile selected: process Extra Fine (design)" "the process read is selected"
ui_expect_new "Extra Fine (design), the design's" "the design's process now"
ui_expect_new "Bench Mini 0.4 nozzle (design), the design's" "and its printer"
ui_expect_new "Profiles details at" "Details… beside the line"
ui_focus_dialog '^Printers and Processes$'
ui_mark
ui_step "Details…"                         ui_click_logged "Profiles details"
ui_expect_new "Import details: Imported mount.3mf (Bambu Studio) into this design." "the import's report"
ui_focus_dialog '^Import Details$'
ui_step "close the report (Esc)"           ui_key Escape
ui_expect_new "Import details closed" "the report closed"
ui_focus_dialog '^Printers and Processes$'
ui_step "close"                            ui_click_logged "Profiles close"
ui_focus_main
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Import Print Settings mount.3mf" "one undo step takes the import back"

echo "--- 2e. 3D Print: New process… twice makes Process 1 and Process 2, the action below a separator (mitcad#828)"
# new_process n: New process…, the box's last entry (Down past every
# profile and the separator), makes Process n, the design's at once, and
# opens it in Printers and Processes, which is closed again.
# open_process_picker: a click on Process opens its picker (mitcad#1108),
# a window of its own that takes the keyboard.
open_process_picker() {
  ui_click_logged "Panel Send to Slicer input process"
  for _ in $(seq 1 40); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Process picker opened: " && break
    sleep 0.2
  done
  ui_focus_dialog '^Process Picker$'
}
new_process() {
  ui_mark
  ui_step "Process"                        open_process_picker
  xdotool key --delay 80 --repeat 30 Down
  ui_step "New process…"                   ui_key Return
  ui_expect_new "3D Print process new: Process $1 (" "New process… made Process $1"
  ui_expect_new "Printers and processes opened" "and opened it"
  ui_focus_dialog '^Printers and Processes$'
  ui_step "close"                          ui_click_logged "Profiles close"
  ui_focus_main
}
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer input process at" "the Process row"
new_process 1
new_process 2
# The box: the profiles by name, then a separator line, then the action.
listed=""
for _ in $(seq 1 30); do
  listed=$(grep -F "Panel Send to Slicer options process: " "$UI_LOG" | tail -1)
  case "$listed" in *"| Process 1 | Process 2 "*"| --- | New process…") break ;; esac
  sleep 0.2
done
case "$listed" in
  *"| Process 1 | Process 2 "*"| --- | New process…") echo "ok   Process 1 and Process 2 listed above the separator and New process…" ;;
  *) ui_fail "the Process box lists '${listed#*: }'" ;;
esac
case "$listed" in *"New Process"*) ui_fail "a new profile named New Process: '${listed#*: }'" ;; esac
# The keyboard skips the separator: one up from New process… is the last
# profile, chosen at once (no window opens, no separator chosen).
ui_mark
open_process_picker
xdotool key --delay 80 --repeat 30 Down
xdotool key --delay 80 Up
ui_key Return
ui_focus_main
ui_expect_new "3D Print process: " "one up from New process… is chosen at once"
ui_sync
chosen=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D Print process: " | tail -1)
chosen=${chosen#*3D Print process: }
case "$chosen" in
  __*|*" "*|"") ui_fail "Up from New process… chose '$chosen', not a profile" ;;
  */*) ;;
  *) ui_fail "Up from New process… chose '$chosen', not a profile" ;;
esac
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Printers and processes opened"; then
  ui_fail "Up from New process… ran New process… again"
fi
echo "ok   the separator is skipped: $chosen chosen"
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"
# Back to the design as it was: the choice and the two new processes.
ui_step "undo the choice"                  ui_key ctrl+z
ui_step "undo Process 2"                   ui_key ctrl+z
ui_expect_new "Undo: Create Profile Process 2" "Process 2 was one undo step"
ui_step "undo Process 1"                   ui_key ctrl+z
ui_expect_new "Undo: Create Profile Process 1" "Process 1 too"

echo "--- 3. Materials: Duplicate of PLA in this design, its name and nozzle temperature; Material… assigns it"
ui_mark
ui_command "Edit Materials"
ui_expect_new "Materials opened" "the window opened"
ui_expect_new "Materials row: fdm_plastics/pla PLA Plastic (mitcad)" "PLA is Mitcad's own"
ui_expect_new "Materials row: metals/steel Steel (mitcad)" "Steel is listed by its class, a metal"
ui_expect_new "Materials item PLA Plastic at" "its row has its place"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "PLA"                              ui_click_logged "Materials item PLA Plastic"
ui_expect_new "Material selected: PLA Plastic (mitcad)" "PLA is selected"
ui_step "Duplicate"                        ui_click_logged "Materials duplicate"
ui_expect_new "Material duplicated: PLA Plastic as " "the user's own copy in this design"
ui_expect_new "Materials save_to at" "Save to for the copy, in no library yet"
ui_sync
ui_mark
xdotool type --delay 40 "Bench PLA"
ui_key Return
ui_expect_new "name = Bench PLA" "Enter renamed it"
ui_expect_new "Materials row: " "the list again"
ui_expect_new "Materials settings tab filament at" "the print tabs beside General"
ui_mark
ui_step "the Filament tab"                 ui_click_logged "Materials settings tab filament"
ui_expect_new "Materials settings search at" "Search settings on a print tab, the window's one search"
ui_mark
ui_step "search 'nozzle'"                  ui_click_logged "Materials settings search"
xdotool type --delay 40 "temperature"
ui_sync
ui_expect_new "Materials settings field temperature at" "the search finds the nozzle temperature"
ui_step "the field (Slicer default)"       ui_click_logged "Materials settings field temperature"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "215"
ui_key Return
ui_expect_new "Material Bench PLA (design) temperature = 215" "Enter saved it"
ui_step "close"                            ui_click_logged "Materials close"
ui_focus_main
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Material > More..."     ui_menu_choose "Material" "More..."
ui_expect_new "Material tool for bodies F1.b0" "the Material tool for Body1 (mitcad#1059)"
ui_step "the picker: Bench PLA"            ui_picker_choose "Panel Material input material" "Material picker" "Bench PLA"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "set for 1 body(ies), appearance replace" "assigned, one undo step"

echo "--- 4. Property Sets: a body's print settings as a summary; Own settings opens its own values"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Property Sets..."          ui_menu_choose "Property Sets..."
ui_expect_new "Property sets row: print Print Process/layer_height = 0.2 mm (profile Standard" \
  "the layer height with its plain name, from the root's profile"
ui_expect_new "Property sets row: print Print Process/fill_density = 25 % (profile Standard" \
  "the infill density as the editor set it"
ui_expect_new "Property sets row: print Print Filament/material = Bench PLA" "the filament from the body's material"
ui_expect_new "Property sets row: print Print Process/own_settings = None" "Body1 has no own process values yet"
# The print rows are below the levels, out of sight where the window is
# short: the table's type-ahead goes to the Own settings row, which brings
# its pencil into view. Where the rows fit, the pencil was logged with the
# others when the window opened (since the mark of the right-click).
ui_focus_dialog '^Property Sets$'
ui_step "go to Own settings (type-ahead)"  xdotool type --delay 40 "Own settings"
ui_sync
ui_expect_new "Property sets print Print Process/own_settings edit at" "the pencil of Own settings"
expect_in_value_column "Property sets print Print Process/own_settings edit" "Own settings' pencil in the Value column"
ui_mark
ui_step "Own settings' pencil"             ui_click_logged "Property sets print Print Process/own_settings edit"
ui_expect_new "Own settings opened: Body1 · Own settings" "Body1's own process settings in the settings form"
ui_expect_new "Own settings search at" "the form's search"
ui_focus_dialog ' Own settings$'
ui_mark
ui_step "search 'walls'"                   ui_click_logged "Own settings search"
xdotool type --delay 40 "walls"
ui_sync
ui_expect_new "Own settings field perimeters at" "the search finds Walls"
ui_step "the field"                        ui_click_logged "Own settings field perimeters"
ui_key ctrl+a
xdotool type --delay 40 "5"
ui_key Return
ui_expect_new "Own setting F1.b0 perimeters = 5" "the body's own value, one model command"
ui_expect_new "Property sets row: print Print Process/perimeters = 5 (body Body1)" "Property Sets sums it up"
ui_expect_new "Own settings clear perimeters at" "↺ beside it"
save
expect_file '[.properties[] | select(.owner == "F1.b0") | .sets[0].properties[0] | [.key, .value]]' \
  '[["perimeters",5]]' "the file keeps the body's own walls"
ui_focus_dialog ' Own settings$'
ui_mark
ui_step "↺"                                ui_click_logged "Own settings clear perimeters"
ui_expect_new "Own setting F1.b0 perimeters = cleared" "cleared"
ui_expect_new "Property sets row: print Print Process/perimeters = 3 (profile Standard" "the profile's again"
ui_step "close"                            ui_click_logged "Own settings close"
ui_focus_dialog '^Property Sets$'
ui_step "close"                            ui_click_logged "Property sets close"
ui_focus_main

echo "--- 4b. Property Sets of the design: a right click on Process's ↺ does nothing (mitcad#761)"
ui_mark
opened=$UI_MARK # the dialog's places are logged from here
ui_step "right-click the top row"          ui_click_logged "Browser Root" 3
ui_expect_new "Context menu: " "its menu opened"
ui_step "choose Property Sets..."          ui_menu_choose "Property Sets..."
ui_expect_new "Property sets row: print Print Process/profile = Standard (chosen on" \
  "the design's own process choice"
ui_focus_dialog '^Property Sets$'
# A Value column of about 150 pixels elides the printer's long name: the
# pencils follow the name as shown and stay inside the column (mitcad#827).
read -r from to <<< "$(value_column)"
[ -n "$from" ] && [ -n "$to" ] || ui_fail "the Value column's place is not logged"
if [ $((to - from)) -gt 150 ]; then
  dialog=$(xdotool search --onlyvisible --name '^Property Sets$' | head -1)
  width=$(xdotool getwindowgeometry --shell "$dialog" | sed -n 's/^WIDTH=//p')
  height=$(xdotool getwindowgeometry --shell "$dialog" | sed -n 's/^HEIGHT=//p')
  ui_mark
  ui_step "a narrower window"              xdotool windowsize "$dialog" $((width - (to - from - 150) * UI_SCALE)) "$height"
  ui_sync
  ui_expect_new "Property sets value column from " "the places logged again"
fi
ui_mark
ui_step "go to Printer (type-ahead)"       xdotool type --delay 40 "Printer"
ui_sync
ui_expect_log "Property sets print Print Printer/profile edit at" "the pencil after the design's printer"
expect_in_value_column "Property sets print Print Printer/profile edit" "Printer's pencil in the Value column"
# The type-ahead's next word starts afresh.
sleep 0.5
# Below the levels, out of sight where the window is short: the type-ahead
# brings the Process row into view (its glyphs are logged again then).
ui_step "go to Process (type-ahead)"       xdotool type --delay 40 "Process"
ui_sync
ui_expect_log "Property sets print Print Process/profile clear at" "↺ after the design's process"
expect_in_value_column "Property sets print Print Process/profile edit" "Process's pencil in the Value column"
expect_in_value_column "Property sets print Print Process/profile clear" "Process's ↺ in the Value column"
ui_mark
ui_step "right-click ↺"                    ui_click_logged "Property sets print Print Process/profile clear" 3
ui_sync
# Nothing is to happen: a short wait for a stray reaction (and for the
# type-ahead's next word to start afresh).
sleep 0.5
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Property sets print Print Process/profile clear$|Property .* cleared"; then
  ui_fail "a right click ran ↺: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "clear$|cleared" | head -3)"
fi
echo "ok   a right click on ↺ runs nothing"
ui_step "go to Material (type-ahead)"      xdotool type --delay 40 "Material"
ui_sync
# Logged since the dialog opened: again only where the type-ahead scrolled
# it into view, else where the dialog showed it at first.
UI_MARK=$opened
ui_expect_new "Property sets print Print Filament/material value at" "the Material row's value"
ui_mark
ui_step "Material"                         ui_click_logged "Property sets print Print Filament/material value"
ui_expect_new "Materials opened" "the next left click opens Materials"
ui_expect_new "Materials close at" "Materials laid out"
ui_focus_dialog '^Materials$'
ui_step "close"                            ui_click_logged "Materials close"
ui_focus_dialog '^Property Sets$'
# Each pencil does its own action, where the log places it (mitcad#827).
expect_in_value_column "Property sets print Print Filament/material edit" "Material's pencil in the Value column"
ui_mark
ui_step "Material's pencil"                ui_click_logged "Property sets print Print Filament/material edit"
ui_expect_new "Property sets print Print Filament/material edit" "the pencil's click"
ui_expect_new "Materials opened" "Material's pencil opens Materials"
ui_focus_dialog '^Materials$'
ui_step "close"                            ui_click_logged "Materials close"
ui_focus_dialog '^Property Sets$'
sleep 0.5
ui_step "go to Process (type-ahead)"       xdotool type --delay 40 "Process"
ui_sync
ui_mark
ui_step "Process's pencil"                 ui_click_logged "Property sets print Print Process/profile edit"
ui_expect_new "Printers and processes opened" "Process's pencil opens Printers and Processes"
ui_expect_new "Profile selected: process Standard" "on the design's process"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Editing property print Print Process/profile"; then
  ui_fail "the pencil also opened the drop-down"
fi
echo "ok   the pencil opened no drop-down"
# A second process of this section's own for the drop-down to offer
# (whatever the sections before left): New › Process, the design's at once,
# one undo step.
ui_focus_dialog '^Printers and Processes$'
ui_mark
ui_step "New"                              ui_click_logged "Profiles new"
ui_expect_new "Profiles new menu: Printer | Process" "a printer or a process"
ui_key Down Down Return
ui_expect_new "Profile created: process " "a second process in this design"
ui_expect_new "Profile shown: process " "the window shows it"
second=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Profile shown: process " | tail -1 |
  sed 's/.*Profile shown: process \(.*\) (design).*/\1/')
[ -n "$second" ] || ui_fail "no name of the second process"
ui_expect_new "Property sets row: print Print Process/profile = $second" "Property Sets shows it as the design's"
ui_focus_dialog '^Printers and Processes$'
ui_step "close"                            ui_click_logged "Profiles close"
ui_focus_dialog '^Property Sets$'
ui_sync
# Process's drop-down (mitcad#827): closed without a change, the editor
# ends; another process chosen is set and ends it too.
ui_mark
ui_step "Process's value"                  ui_click_logged "Property sets print Print Process/profile value"
ui_expect_new "Property sets profile list: " "the drop-down of the processes"
ui_step "close the list (Esc)"             ui_key Escape
ui_expect_new "Property sets profile list closed" "closed without a change, the editor ends"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Property print Print Process/profile set: "; then
  ui_fail "closing the list set a process"
fi
echo "ok   and nothing was set"
list=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Property sets profile list: " | tail -1 |
  sed 's/.*Property sets profile list: //')
index=${list##*"(current "}
index=${index%")"}
mapfile -t processes < <(sed 's/ | /\n/g' <<< "${list% "(current "*}")
# The nearest process of another name, below the current one or above it.
step=0
for ((i = index + 1; i < ${#processes[@]}; i++)); do
  [ "${processes[$i]}" != "${processes[$index]}" ] && { step=$((i - index)); break; }
done
if [ "$step" -eq 0 ]; then
  for ((i = index - 1; i >= 0; i--)); do
    [ "${processes[$i]}" != "${processes[$index]}" ] && { step=$((i - index)); break; }
  done
fi
[ "$step" -ne 0 ] || ui_fail "no other process to choose: $list"
other=${processes[$((index + step))]}
ui_mark
ui_step "choose $other"                    ui_choose "Property sets print Print Process/profile value" "$step"
ui_expect_new "Property sets profile list closed" "the list closed, the editor ends"
ui_expect_new "Property print Print Process/profile set: $other" "the process chosen is set"
# Back to the design as it was: the choice and the second process.
ui_focus_main
ui_mark
ui_step "undo the choice"                  ui_key ctrl+z
ui_expect_new "Undo: " "the choice was one undo step"
ui_mark
ui_step "undo the second process"          ui_key ctrl+z
ui_expect_new "Undo: " "the second process was one undo step"
ui_focus_dialog '^Property Sets$'
ui_step "close"                            ui_click_logged "Property sets close"
ui_focus_main

echo "--- 5. Show Build Volume, and 3D Print to QIDI Studio"
ui_mark
ui_step "Show Build Volume (search)"       ui_command "Show Build Volume"
ui_expect_new "Show build volume on" "on"
ui_expect_new "Build volume: Bench X 0.4 nozzle 256 x 256 x 250 mm, outside: Body2" \
  "the printer's bed and height; the far box does not fit"
rm -f "$ARGS"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "3D Print printer: Bench X 0.4 nozzle" "the panel shows the design's printer"
ui_key Return
ui_expect_new "3D Print: print settings for qidi_studio, filaments 1" "QIDI Studio's settings, one filament"
for _ in $(seq 1 50); do
  [ -f "$ARGS" ] && break
  sleep 0.2
done
[ -f "$ARGS" ] || ui_fail "the slicer was not started"
grep -q '\.3mf$' "$ARGS" || ui_fail "the slicer got no 3MF: $(cat "$ARGS")"
echo "ok   the slicer got the 3MF"

echo "--- 6. A Bambu Studio project: nothing left unread (mitcad#654)"
ui_focus_main
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer action import.0 at" "Import from slicer… is shown"
ui_step "Import from slicer…"              ui_click_logged "Panel Send to Slicer action import.0"
ui_type_path "Import from Slicer" "$WORK/mount.3mf"
ui_focus_main
ui_expect_new "Imported print settings from mount.3mf: printers: Bench Mini 0.4 nozzle; processes: Extra Fine" \
  "the project's printer and process came in"
ui_expect_new "3D Print import: mount.3mf imported" \
  "the line names the file, no names of materials"
summary=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D Print import: mount.3mf imported" | tail -1)
case $summary in
  *"not read"*) ui_fail "95% was not read: $summary" ;;
esac
echo "ok   every setting was read, 95% too"
ui_mark
ui_step "Details…"                         ui_click_logged "Panel Send to Slicer action import.1"
ui_expect_new "Import details: Imported mount.3mf (Bambu Studio) into this design." "its first line"
ui_expect_new "Import details: Process: Extra Fine" "the process by name"
ui_focus_dialog '^Import Details$'
ui_step "close the report (Esc)"           ui_key Escape
ui_expect_new "Import details closed" "the report closed"
ui_focus_main
ui_step "Cancel the panel (Esc)"           ui_key Escape

echo "--- 7. Send at once, while the panel's question waits (mitcad#888)"
# The exports made slow; 3D Print chosen in the search and sent by Enter
# in one go, before the panel's deferred question about changed library
# profiles ran: the export's job runs it, and it waits for the job's end
# instead of reading the document. Up to three tries, for keys that come
# apart.
ui_stop_app
MITCAD_TEST_JOB_SLEEP_MS=export=1500 ui_start_app --open "$FILE" --no-native-dialogs
deferred=0
for attempt in 1 2 3; do
  ui_focus_main
  ui_mark
  ui_step "Send to Slicer search"          ui_key s
  xdotool type --delay 40 "Send to Slicer"
  ui_sync
  xdotool key --delay 1 Return Return
  ui_expect_new "3D Print last line: " "try $attempt: sent" 60
  grep -q "The document was read while a job computes it" "$UI_LOG" && ui_fail "the document was read during the export"
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "3D Print: library changes asked after the job"; then
    deferred=1
    break
  fi
  echo "note: try $attempt: the question ran before the export"
done
[ "$deferred" = 1 ] || ui_fail "the export never met the panel's question in three tries"
echo "ok   the question waited for the export's end"

ui_finish "UI print settings"
