#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/browser/LibraryPicker.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/Dialogs.cpp core/model/src/print_library.rs core/model/src/document/library_items.rs
# check-all sources: tools/cli tools/libraries/print-library
# Hidden built-ins (mitcad#1270) through the real UI. The base library is
# a local one of the test's own (MITCAD_LIBRARIES_DIR) with two of
# MitCad's built-in printers, Bambu Lab A1 (0.4 nozzle) and Bambu Lab A1
# mini (0.4 nozzle), from tools/libraries/print-library; My library is the
# test's own folder (MITCAD_PRINT_LIBRARY_DIR), empty at first. The hidden
# ids are a setting of the test's own settings folder (XDG_CONFIG_HOME).
#   1. The Printers manager on a box: Built-in 2, open. Bambu Lab A1
#      selected, Hide: it is gone from the list (Built-in 1), the setting
#      names it, and Show hidden appears.
#   2. Send to Slicer: the design's Printer box lists Bambu Lab A1 mini,
#      not Bambu Lab A1.
#   3. The Printers manager again: Show hidden lists it grayed (Built-in 2).
#   4. Its row's context menu: Duplicate | Unhide; Unhide restores it (the
#      setting no longer names it). The context menu's Hide hides it again.
#   5. A design that uses Bambu Lab A1 (its copy, made with mitcad-cli)
#      still shows it as its own: This design's, the design's printer; its
#      Printer box lists it.
#   6. With a printer in My library (Bench X), the Printers manager opens
#      with Built-in folded, and the Printer picker lists the built-ins
#      last, under Built-in.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hide-built-ins-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-hide-built-ins.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/box.mitcad
USED=$WORK/used.mitcad
BASE=$WORK/base-library
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
A1="Bambu Lab A1 (0.4 nozzle)"
MINI="Bambu Lab A1 mini (0.4 nozzle)"
# The user's library and the fetched libraries in the test's own folders.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
export MITCAD_LIBRARIES_DIR=$WORK/libraries
mkdir -p "$MITCAD_PRINT_LIBRARY_DIR" "$MITCAD_LIBRARIES_DIR"

# cli script [args...]: mitcad-cli run, failing the test when it fails.
cli() {
  "$CLI" run "$@" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli run $*"; }
}

# logged_after "text": the last line since the mark with the text.
logged_after() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1
}

# not_logged_after "text" description: fails when a line since the mark
# has the text.
not_logged_after() {
  ui_sync
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1"; then
    ui_fail "$2: $(logged_after "$1")"
  fi
  echo "ok   $2"
}

# hidden_setting: the setting's hidden printers ("" without any).
hidden_setting() {
  [ -f "$SETTINGS" ] || return 0
  sed -n '/^\[printLibrary\]/,/^\[/s/^hidden_printer=//p' "$SETTINGS"
}

# The base library: MitCad's component library id with two of its
# printers, recorded with git and fetched as MitCad fetches it.
"$CLI" library init "$BASE" --id mitcad-parts --name 'Print fixtures' --license MIT > "$WORK/setup.log" 2>&1 ||
  { cat "$WORK/setup.log"; ui_fail "mitcad-cli library init"; }
mkdir -p "$BASE/print/printers"
cp "$ROOT/tools/libraries/print-library/print/printers/bambu-lab-a1.json" \
  "$ROOT/tools/libraries/print-library/print/printers/bambu-lab-a1-mini.json" "$BASE/print/printers/"
jq '.library_version = "1.0.0"
    | .printers = [{"id": "bambu-lab-a1", "path": "print/printers/bambu-lab-a1.json"},
                   {"id": "bambu-lab-a1-mini", "path": "print/printers/bambu-lab-a1-mini.json"}]' \
  "$BASE/mitcad-library.json" > "$WORK/manifest.json" || ui_fail "the base library's manifest"
mv "$WORK/manifest.json" "$BASE/mitcad-library.json"
git -C "$BASE" -c user.name='Mitcad developers' -c user.email=devs@mitcad.org add -A
git -C "$BASE" -c user.name='Mitcad developers' -c user.email=devs@mitcad.org commit -qm 'Built-in printers' ||
  ui_fail "git commit (the base library)"
git -C "$BASE" tag v1.0.0
"$CLI" library fetch "$BASE" >> "$WORK/setup.log" 2>&1 || { cat "$WORK/setup.log"; ui_fail "mitcad-cli library fetch"; }

# The designs: a box, and a box whose printer is Bambu Lab A1 (its copy).
cat > "$WORK/box.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}}
]
EOF
cli "$WORK/box.json" --save "$FILE"
cat > "$WORK/used.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "bambu-lab-a1", "library": "base"}
]
EOF
cli "$WORK/used.json" --save "$USED"

# 3MF, whose Printer row is enabled.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nformat=3mf\n' > "$SETTINGS"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the box is in the view"

echo "--- 1. Printers: Bambu Lab A1 hidden"
ui_mark
ui_command "Printers"
ui_expect_new "Printers opened" "the Printers manager opened"
ui_expect_new "Printers groups: This design 0 | My library 0 | Built-in 2" "the two built-in printers"
ui_expect_new "Printers folded: none" "no printer of the user's: Built-in open"
ui_expect_new "Printers mitcad item $A1 at" "Bambu Lab A1 has its place"
ui_focus_dialog '^Printers$'
ui_mark
ui_step "Bambu Lab A1"                     ui_click_logged "Printers mitcad item $A1"
ui_expect_new "Profile selected: printer $A1 (mitcad)" "Bambu Lab A1 selected"
ui_expect_new "Printers hide at" "Hide offered for a built-in printer"
ui_mark
ui_step "Hide"                             ui_click_logged "Printers hide"
ui_expect_new "Printers hidden: $A1" "hidden"
ui_expect_new "Printers groups: This design 0 | My library 0 | Built-in 1" "gone from the list"
ui_expect_new "Printers row: printer/bambu-lab-a1 $A1 (mitcad), hidden" "its row hidden"
ui_expect_new "Printers show_hidden at" "Show hidden appears"
not_logged_after "Printers mitcad item $A1 at" "no place of Bambu Lab A1 in the list"
[ "$(hidden_setting)" = bambu-lab-a1 ] || ui_fail "the setting names '$(hidden_setting)', not bambu-lab-a1"
echo "ok   the user's setting names it"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Printers closed" "closed"

echo "--- 2. Send to Slicer: the Printer box without it"
ui_focus_main
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer options printer: " "the Printer box's options"
options=$(logged_after "Panel Send to Slicer options printer: ")
case "$options" in
  *"$A1"*) ui_fail "the Printer box still lists $A1: ${options#*: }" ;;
esac
case "$options" in
  *"$MINI"*) echo "ok   the Printer box lists $MINI, not $A1" ;;
  *) ui_fail "the Printer box does not list $MINI: ${options#*: }" ;;
esac
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"

echo "--- 3. Show hidden: listed grayed"
ui_mark
ui_command "Printers"
ui_expect_new "Printers opened" "the Printers manager opened"
ui_expect_new "Printers show_hidden at" "Show hidden under the list"
ui_focus_dialog '^Printers$'
ui_mark
ui_step "Show hidden"                      ui_click_logged "Printers show_hidden"
ui_expect_new "Printers show hidden: on" "Show hidden on"
ui_expect_new "Printers groups: This design 0 | My library 0 | Built-in 2" "listed again"
ui_expect_new "Printers grayed: $A1" "grayed"
ui_expect_new "Printers mitcad item $A1 at" "its place in the list"

echo "--- 4. The context menu: Unhide, then Hide"
ui_mark
ui_step "its context menu"                 ui_click_logged "Printers mitcad item $A1" 3
ui_expect_new "Context menu: Duplicate | Unhide" "Duplicate and Unhide"
ui_menu_choose "Unhide"
ui_expect_new "Printers unhidden: $A1" "unhidden"
ui_expect_new "Printers grayed: none" "no longer grayed"
[ -z "$(hidden_setting)" ] || ui_fail "the setting still names '$(hidden_setting)'"
echo "ok   the setting names none"
ui_expect_new "Printers mitcad item $A1 at" "listed"
ui_mark
ui_step "its context menu"                 ui_click_logged "Printers mitcad item $A1" 3
ui_expect_new "Context menu: Duplicate | Hide" "Duplicate and Hide"
ui_menu_choose "Hide"
ui_expect_new "Printers hidden: $A1" "hidden again"
ui_expect_new "Printers grayed: $A1" "grayed while Show hidden is on"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Printers closed" "closed"

echo "--- 5. A design that uses it keeps it as its own"
ui_stop_app
ui_start_app --open "$USED" --no-native-dialogs
ui_mark
ui_command "Printers"
ui_expect_new "Printers opened" "the Printers manager opened"
ui_expect_new "Printers groups: This design 1 | My library 0 | Built-in 1" "This design's copy, the built-in hidden"
ui_expect_new "$A1 (design), the design's" "This design's printer"
ui_focus_dialog '^Printers$'
ui_step "close (Esc)"                      ui_key Escape
ui_focus_main
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer options printer: " "the Printer box's options"
options=$(logged_after "Panel Send to Slicer options printer: ")
case "$options" in
  *"$A1"*) echo "ok   the Printer box lists the design's $A1" ;;
  *) ui_fail "the Printer box does not list the design's $A1: ${options#*: }" ;;
esac
ui_expect_new "Panel Send to Slicer input printer shows: " "the field shows the design's printer"
shown=$(logged_after "Panel Send to Slicer input printer shows: ")
case "$shown" in
  *"A1 "*) echo "ok   ${shown#*Panel }" ;;
  *) ui_fail "the field shows '${shown#*shows: }'" ;;
esac
ui_mark
ui_step "close the panel (Esc)"            ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "the panel closed"

echo "--- 6. A printer of My library: Built-in folded, the built-ins last"
ui_stop_app
cat > "$WORK/library.json" << 'EOF'
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench X 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}}
]
EOF
cli "$WORK/library.json" --save "$WORK/scratch.mitcad"
ui_start_app --open "$FILE" --no-native-dialogs
ui_mark
ui_command "Printers"
ui_expect_new "Printers opened" "the Printers manager opened"
ui_expect_new "Printers groups: This design 0 | My library 1 | Built-in 1" "My library's Bench X"
ui_expect_new "Printers folded: Built-in" "Built-in starts folded"
ui_expect_new "Printers user item Bench X 0.4 nozzle at" "My library's printer in sight"
not_logged_after "Printers mitcad item " "no built-in row in sight"
ui_focus_dialog '^Printers$'
ui_step "close (Esc)"                      ui_key Escape
ui_focus_main
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer input printer at" "the Printer row"
ui_mark
ui_step "open Printer"                     ui_click_logged "Panel Send to Slicer input printer"
ui_expect_new "Printer picker sections: " "the picker's sections"
sections=$(logged_after "Printer picker sections: ")
case "$sections" in
  *"| Built-in 1") echo "ok   the built-ins last: ${sections#*: }" ;;
  *) ui_fail "the sections are '${sections#*: }'" ;;
esac
ui_focus_dialog '^Printer Picker$'
ui_step "close (Esc)"                      ui_key Escape
ui_focus_main
ui_step "close the panel (Esc)"            ui_key Escape

ui_finish "UI hide built-ins"
