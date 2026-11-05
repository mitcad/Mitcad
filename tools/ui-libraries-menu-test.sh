#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowLibraries.cpp app/files/MainWindowPrint.cpp app/MainWindow.cpp
# check-all sources: app/print app/browser/BrowserController.cpp app/commands/MaterialCommands.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs tools/cli
# Tools › Libraries (mitcad#1250, GitHub #132) through the real UI. My
# library is the test's own folder (MITCAD_PRINT_LIBRARY_DIR) with a
# material Shop PETG and a printer Bench X.
#   1. With no design open (the empty Untitled window): Tools › Libraries
#      lists, in its groups, Insert from Library…, Community Library… |
#      Components in This Design…, Publish to Library… | Material Library…,
#      Printer Library…, Process Library… | Library Sources….
#   2. Material Library… (its mnemonic): the Materials manager on the
#      libraries, My library's Shop PETG selected; its density changed,
#      written to its file.
#   3. Printer Library…: the Printers manager on My library, Bench X
#      selected; renamed Bench X2 in its file. Process Library…: the
#      Processes manager on My library, which has none, nothing selected.
#      The window's design did not change: no modified mark in its title.
#   4. A design of two boxes: Body1 and Body2 picked, the Material tool:
#      its Material Library… opens the manager on My library while the
#      panel stays open; Duplicate offers My library first; the copy,
#      renamed Bench PETG, is offered by the tool once the manager closes
#      ("new: Bench PETG"), and OK gives it to both bodies, still picked.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-libraries-menu-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-libraries-menu.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/boxes.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
LIB=$MITCAD_PRINT_LIBRARY_DIR

# expect_item path filter expected description: a jq filter on a library
# item's file.
expect_item() {
  local actual
  [ -f "$1" ] || ui_fail "$4: no $1"
  actual=$(jq -c "$2" "$1") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

# logged_after "text" sed-expression: the last line since the mark with
# the text, through the expression.
logged_after() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F -- "$1" | tail -1 | sed "$2"
}

# libraries_entry key: Tools › Libraries › the entry of the mnemonic key.
libraries_entry() {
  ui_step "the Tools menu (Alt+T)"         ui_key alt+t
  ui_expect_new "Tools menu: " "the Tools menu opened"
  ui_step "Libraries (L)"                  ui_key l
  ui_expect_new "Libraries menu: " "the Libraries menu opened"
  ui_step "the entry ($1)"                 ui_key "$1"
}

# My library: the material Shop PETG (an item file as Save to writes it)
# and the printer Bench X.
mkdir -p "$LIB/materials"
cat > "$LIB/materials/shop_petg.json" << 'EOF'
{
  "format": "mitcad-print-item",
  "version": 1,
  "material": {
    "id": "shop_petg",
    "name": "Shop PETG",
    "density": 1.27,
    "appearance": "plastic_black",
    "filament": {"filament_type": "PETG", "temperature": 240},
    "version": 1
  }
}
EOF
cat > "$WORK/printer.json" << 'EOF'
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench X",
   "values": {"bed_shape": [[0, 0], [220, 0], [220, 220], [0, 220]], "max_print_height": 250,
              "nozzle_diameter": 0.4}}
]
EOF
"$CLI" run "$WORK/printer.json" --save "$WORK/other.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (Bench X into My library)"; }
[ -f "$LIB/print/printers/bench-x.json" ] || ui_fail "Bench X is not in My library"
# The design of part 4: two boxes of the default material.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
# No design open: the empty Untitled window.
ui_start_app --no-native-dialogs
untitled=$(ui_window_title)

echo "--- 1. Tools › Libraries in its groups"
ui_focus_main
ui_mark
libraries_entry m
menu=$(logged_after "Libraries menu: " 's/.*Libraries menu: //')
expected="Insert from Library… | Community Library… | - | Components in This Design… | Publish to Library… | - | \
Material Library… | Printer Library… | Process Library… | - | Library Sources…"
[ "$menu" = "$expected" ] || ui_fail "the Libraries menu is '$menu', expected '$expected'"
echo "ok   the Libraries menu in its four groups: $menu"

echo "--- 2. Material Library… without a design: My library, Shop PETG edited"
ui_expect_new "Materials opened without a design" "Material Library… opened the Materials manager on the libraries"
ui_expect_new "Materials loaded: " "its list filled when the items were read"
ui_expect_new "Materials groups: My library 1 | Built-in " "My library and Built-in, no This design"
ui_expect_new "Materials on My library: Shop PETG" "on My library, its first material selected"
ui_expect_new "Material selected: Shop PETG (user)" "Shop PETG is selected"
ui_expect_new "Materials editing: Editing: My library — every design of yours can use it" "edited in My library"
ui_expect_new "Materials density at" "Density has its place"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "Density"                          ui_click_logged "Materials density"
ui_key ctrl+a
xdotool type --delay 40 "1.3"
ui_key Return
ui_expect_new "Material Shop PETG (user) density = 1.3" "Enter saved the density"
expect_item "$LIB/materials/shop_petg.json" '.material.density' '1.3' "My library's file has the new density"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Materials closed after changes" "closed"

echo "--- 3. Printer Library… and Process Library… without a design"
ui_focus_main
ui_mark
libraries_entry r
ui_expect_new "Printers opened without a design" "Printer Library… opened the Printers manager on the libraries"
ui_expect_new "Printers groups: My library 1 | Built-in " "My library and Built-in, no This design"
ui_expect_new "Printers on My library: Bench X" "on My library, Bench X selected"
ui_expect_new "Profile selected: printer Bench X (user)" "Bench X is selected"
ui_expect_new "Printers name at" "its name has its place"
ui_focus_dialog '^Printers$'
ui_mark
ui_step "its name"                         ui_click_logged "Printers name"
ui_key ctrl+a
xdotool type --delay 40 "Bench X2"
ui_key Return
ui_expect_new "Profile Bench X (user) name = Bench X2" "renamed in My library"
expect_item "$LIB/print/printers/bench-x.json" '.profile.name' '"Bench X2"' "My library's file has the new name"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Printers closed after changes" "closed"
ui_focus_main
ui_mark
libraries_entry o
ui_expect_new "Processes opened without a design" "Process Library… opened the Processes manager on the libraries"
ui_expect_new "Processes on My library: nothing selected" "on My library, which has no process"
ui_focus_dialog '^Processes$'
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Processes closed" "closed"
ui_focus_main
[ "$(ui_window_title)" = "$untitled" ] || ui_fail "the window's design changed: '$(ui_window_title)'"
echo "ok   the window's design did not change: $untitled"

echo "--- 4. The Material tool's Material Library…: a material made there is offered, the bodies kept"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/Bodies/Body2 at" "the browser lists the bodies"
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Browser selected: Root/Bodies/Body1" "Body1 selected"
ui_step "Ctrl+click Body2"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body2") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2" "Body1 and Body2 selected"
ui_mark
ui_step "Material (search)"                ui_command "Material"
ui_expect_new "Command search chose solid.material" "the search found the Material tool"
ui_expect_new "Panel Material bodies note: 2 bodies: Steel" "the panel has both bodies"
ui_expect_new "Panel Material action library.0 at" "Material Library… under the picker"
ui_mark
ui_step "Material Library…"                ui_click_logged "Panel Material action library.0"
ui_expect_new "Material tool: Material Library opened" "the tool opened the manager"
ui_expect_new "Materials opened" "the Materials manager opened"
ui_expect_new "Materials on My library: Shop PETG" "on My library, Shop PETG selected"
ui_expect_new "Materials duplicate at" "Duplicate under the list"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "Duplicate"                        ui_click_logged "Materials duplicate"
ui_expect_new "Materials duplicate menu: My library | This design" "Duplicate offers My library first"
# The menu opens with nothing active: Down takes its first entry.
ui_key Down Return
ui_expect_new "Material duplicated: Shop PETG as " "a copy made"
place=$(logged_after "Material duplicated: Shop PETG as " 's/.* as .* in \(.*\)$/\1/')
[ "$place" = user ] || ui_fail "the copy went to '$place', not My library"
echo "ok   into My library"
ui_sync
ui_mark
xdotool type --delay 40 "Bench PETG"
ui_key Return
ui_expect_new "Material Shop PETG (copy) (user) name = Bench PETG" "Enter renamed it"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Materials closed" "closed"
ui_expect_new "Material tool: materials read again, " "the tool read its materials again"
ui_expect_new ", new: Bench PETG" "Bench PETG is offered"
ui_focus_main
ui_expect_log "Panel Material input material at" "the picker button's place is logged"
ui_step "the picker: Bench PETG"           ui_picker_choose "Panel Material input material" "Material picker" "Bench PETG"
ui_expect_new "Panel Material tile material: Bench PETG (My library)" "the picker button shows Bench PETG"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material tool: Bench PETG/" "the tool's log line"
ui_expect_new " to 2 bodies" "both bodies kept and given Bench PETG"

ui_finish "UI Tools › Libraries"
