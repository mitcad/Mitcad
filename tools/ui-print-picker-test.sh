#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print/ProfilePickerRows.cpp app/browser/LibraryPicker.cpp app/framework/CommandPanel.cpp
# check-all sources: app/files/MainWindowPrint.cpp core/model/src/document/library_items.rs tools/cli
# check-all sources: app/framework/PickerComboBox.cpp
# Send to Slicer's Printer picker (mitcad#1108) through the real UI, on a
# design that imported a printer from a 3MF (plate.3mf) whose name a
# printer of My library has too, beside two more printers there:
#   1. Printer opens the picker (a window of its own, "Printer Picker"):
#      the printers under their vendor, Acme Lab 3 and Bench 1, each by its
#      model and nozzle; Escape closes it choosing nothing.
#   2. "X1" typed in its search narrows the list to the two X1 rows; Enter
#      chooses the first, the design's own from plate.3mf: the design's
#      printer is set ("Use Printer …", one undo step), and the closed
#      field shows the model and the nozzle without the vendor.
#   3. No button beside Printer or Process (mitcad#1134): the box lists
#      Edit printers… last, below New printer…; chosen in the picker, it
#      opens the Printers manager (mitcad#1135) on the chosen printer and
#      keeps the choice (no undo step).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-picker-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-print-picker.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/plate.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
X1="X1 Carbon · 0.4 mm nozzle"

# The slicer's project: a box with the Acme Lab X1 printer, for OrcaSlicer.
cat > "$WORK/slicer.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "printer", "name": "Acme Lab X1 Carbon 0.4 nozzle",
   "values": {"bed_shape": [[0, 0], [256, 0], [256, 256], [0, 256]], "max_print_height": 250,
              "nozzle_diameter": 0.4, "printer_settings_id": {"orcaslicer": "Acme Lab X1 Carbon 0.4 nozzle"}}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "acme-lab-x1-carbon-0-4-nozzle"},
  {"cmd": "export", "path": "$WORK/plate.3mf", "dialect": "orcaslicer"}
]
EOF
"$CLI" run "$WORK/slicer.json" --save "$WORK/slicer.mitcad" > "$WORK/cli-slicer.log" 2>&1 ||
  { cat "$WORK/cli-slicer.log"; ui_fail "mitcad-cli (the slicer's project)"; }
[ -s "$WORK/plate.3mf" ] || ui_fail "the slicer's project was not written"

# The design: a box and the 3MF's printer, not chosen (Slicer default).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "import_print_profile", "path": "$WORK/plate.3mf", "choose": false}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# My library: a printer of the same name, and two more.
cat > "$WORK/library.json" << 'EOF'
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Acme Lab X1 Carbon 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}},
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Acme Lab P1S 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}},
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench Mini 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}}
]
EOF
"$CLI" run "$WORK/library.json" --save "$WORK/other.mitcad" > "$WORK/cli-library.log" 2>&1 ||
  { cat "$WORK/cli-library.log"; ui_fail "mitcad-cli (the printers of My library)"; }

# 3MF, whose Printer row is enabled.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nformat=3mf\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the box is in the view"

echo "--- 1. Printer: the picker by vendor; Escape closes it"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "3D Print printer: Slicer default" "the design has no printer chosen"
ui_expect_new "Panel Send to Slicer input printer shows: Slicer default" "the field shows Slicer default"
ui_expect_new "Panel Send to Slicer input printer at" "the Printer row"
ui_expect_new "Panel Send to Slicer input process at" "the Process row"
# The panel's places, logged together: the fields only, no pencil beside
# them (mitcad#1134).
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Panel Send to Slicer button_(printer|process) at"; then
  ui_fail "a button beside Printer or Process: $(grep -E "Panel Send to Slicer button_(printer|process) at" "$UI_LOG" | tail -1)"
fi
echo "ok   no button beside Printer or Process"
ui_mark
ui_step "open Printer"                     ui_click_logged "Panel Send to Slicer input printer"
# (The built-in printers of the base library, where this computer has it,
# come with their vendors' headings too.)
ui_expect_new "entries, current Slicer default" "the picker opened on Slicer default"
ui_expect_new "Printer picker sections: Acme Lab 3 |" "the three Acme Lab printers under their vendor"
sections=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Printer picker sections: " | tail -1)
case "$sections" in
  *"| Bench 1"*) echo "ok   Bench's printer under its own vendor" ;;
  *) ui_fail "the sections are '${sections#*: }'" ;;
esac
ui_expect_new "Printer picker item P1S · 0.4 mm nozzle at" "a row is the model and the nozzle, no vendor"
ui_focus_dialog '^Printer Picker$'
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Printer picker closed" "Escape closes the picker"
ui_focus_main
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D Print printer: "; then
  ui_fail "Escape chose a printer"
fi
echo "ok   nothing chosen"

echo "--- 2. \"X1\" typed: the X1 rows; Enter chooses the first"
ui_mark
ui_step "open Printer"                     ui_click_logged "Panel Send to Slicer input printer"
ui_expect_new "Printer picker opened: " "the picker opened"
ui_focus_dialog '^Printer Picker$'
ui_mark
xdotool type --delay 40 -- "X1"
ui_expect_new "Printer picker search: X1, first $X1" "the first X1 row is current"
# The rows in sight after the search: the two X1 printers only (the
# places' last block since typing; "X" alone shows the same rows, so "X1"
# logs no block of its own).
shown=""
for _ in $(seq 1 30); do
  shown=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    awk '/Printer picker search at /{block=""} /Printer picker item /{block=block $0 "\n"} END{printf "%s", block}' |
    sed 's/ at [0-9-]*,[0-9-]*$//')
  [ "$(printf '%s\n' "$shown" | grep -c .)" -ge 2 ] && break
  sleep 0.2
done
# The two Acme Lab X1 printers (and a built-in X1 where there is one).
[ "$(printf '%s\n' "$shown" | grep -c .)" -ge 2 ] || ui_fail "the list after X1 shows '$shown'"
if printf '%s\n' "$shown" | grep -vxF "Printer picker item $X1" | grep -q .; then
  ui_fail "the list after X1 shows other rows: '$shown'"
fi
echo "ok   the list narrowed to the X1 printers"
ui_mark
ui_step "choose (Enter)"                   ui_key Return
ui_expect_new "Printer picker chose: $X1" "Enter chose the first X1 row"
ui_focus_main
ui_expect_new "3D Print printer: design/acme-lab-x1-carbon-0-4-nozzle" \
  "the design's printer is the one from plate.3mf"
ui_expect_new "Panel Send to Slicer input printer shows: $X1" "the field shows the model and the nozzle"

echo "--- 3. Edit printers…, the last row: the Printers manager on the chosen printer (mitcad#1134, mitcad#1135)"
# The box's options as logged: the printers, a line, New printer… and last
# Edit printers….
listed=$(grep -F "Panel Send to Slicer options printer: " "$UI_LOG" | tail -1)
case "$listed" in
  *"| --- | New printer… | Edit printers…") echo "ok   Edit printers… last, below New printer…" ;;
  *) ui_fail "the Printer box lists '${listed#*: }'" ;;
esac
ui_mark
ui_step "open Printer"                     ui_click_logged "Panel Send to Slicer input printer"
ui_expect_new "Printer picker opened: " "the picker opened"
ui_focus_dialog '^Printer Picker$'
ui_mark
xdotool type --delay 40 -- "Edit printers"
ui_expect_new "Printer picker search: Edit printers, first Edit printers…" "the search finds the row"
ui_mark
ui_step "choose (Enter)"                   ui_key Return
ui_expect_new "Printer picker chose: Edit printers…" "Edit printers… chosen"
ui_expect_new "3D Print printer edit: design/acme-lab-x1-carbon-0-4-nozzle" "on the design's printer"
ui_expect_new "Printers opened" "the Printers manager opened"
ui_expect_new "Profile selected: printer Acme Lab X1 Carbon 0.4 nozzle (design)" "on the chosen printer"
ui_focus_dialog '^Printers$'
ui_step "close"                            ui_click_logged "Printers close"
ui_focus_main
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D Print printer: "; then
  ui_fail "Edit printers… changed the printer"
fi
shown=$(grep -F "Panel Send to Slicer input printer shows: " "$UI_LOG" | tail -1)
[ "${shown#*shows: }" = "$X1" ] || ui_fail "the field shows '${shown#*shows: }' after Edit printers…"
echo "ok   the choice stays: the field shows $X1"
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Use Printer Acme Lab X1 Carbon 0.4 nozzle" "the choice was one undo step, kept by Cancel"

ui_finish "UI print picker"
