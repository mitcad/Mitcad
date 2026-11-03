#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserController.cpp app/MainWindow.cpp app/files/MainWindowPrint.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs
# check-all sources: core/model-api/src/print_profiles.rs tools/cli
# The print managers (mitcad#1135) through the real UI, on a design of one
# box in a project. My library is the test's own folder
# (MITCAD_PRINT_LIBRARY_DIR) with a process Bench, the project's library a
# process Draft, the design its own process Mine, and another design of
# the project uses Draft.
#   1. Make › 3D Print › Printers… and Processes… open two windows of
#      their own, both open at once. The Processes manager lists This
#      design, This project, My library and Built-in, each with its count
#      (Mine, Draft, Bench); its search finds across the groups.
#   2. Draft of This project: the editor says "Editing: this project's
#      library — every design of the project uses it"; renamed Draft
#      Fine, its file in the project's library has the name, and the other
#      design of the project sees the change and takes it (mitcad-cli).
#   3. Copy to: Mine of This design into This project (the menu of the
#      other places): its file in the project's library. Import from
#      Slicer into the place chosen in its box, This project: the
#      slicer's process Fine (a fake Bambu Studio folder) in the
#      project's library, shown under This project.
#   4. The Materials manager: the built-in PLA Plastic is read only (the
#      editor's line says so), its Duplicate asks where (This design, This
#      project, My library), My library chosen: its file there.
#   5. Tools has no My Library… (mitcad#1135): My library is a group of
#      each manager.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-managers-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-print-managers.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
PROJ=$WORK/bench-project
FILE=$PROJ/box.mitcad
# The user's library of materials and profiles in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
LIB=$MITCAD_PRINT_LIBRARY_DIR
PROJECT_LIB=$PROJ/print-library

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

# cli script [args...]: mitcad-cli run, failing the test when it fails.
cli() {
  "$CLI" run "$@" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli run $*"; }
}

# The project, My library's Bench and the project's Draft.
"$CLI" project init "$PROJ" --no-history > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli project init"; }
cat > "$WORK/libraries.json" << EOF
[
  {"cmd": "create_print_profile", "library": "user", "kind": "process", "name": "Bench",
   "values": {"layer_height": 0.28}},
  {"cmd": "create_print_profile", "library": "project", "project": "$PROJ", "kind": "process", "name": "Draft",
   "values": {"layer_height": 0.2}}
]
EOF
cli "$WORK/libraries.json" --save "$WORK/scratch.mitcad"
[ -f "$LIB/print/processes/bench.json" ] || ui_fail "Bench is not in My library"
[ -f "$PROJECT_LIB/print/processes/draft.json" ] || ui_fail "Draft is not in the project's library"
# The design: one box and its own process Mine.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Mine", "choose": true}
]
EOF
cli "$WORK/model.json" --save "$FILE"
# Another design of the project that uses Draft (its copy).
cat > "$WORK/other.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "draft",
   "library": "project", "project": "$PROJ"}
]
EOF
cli "$WORK/other.json" --save "$PROJ/other.mitcad"

# An installed slicer's data folder (mitcad#830) with a process of the
# user's, Fine, for Import from Slicer.
PRESETS=$XDG_CONFIG_HOME/BambuStudio/user/default
mkdir -p "$PRESETS/process"
printf '{"name": "Fine", "from": "User", "layer_height": "0.12"}\n' > "$PRESETS/process/Fine.json"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the box is in the view"

echo "--- 1. Printers… and Processes…: two managers, the four groups"
ui_mark
ui_command "Printers"
ui_expect_new "Printers opened" "the Printers manager opened"
ui_expect_new "Printers groups: This design 0 | This project 0 | My library 0 | Built-in " \
  "This design, This project, My library and Built-in, each with its count"
ui_focus_main
ui_mark
ui_command "Processes"
ui_expect_new "Processes opened" "the Processes manager opened"
ui_expect_new "Processes groups: This design 1 | This project 1 | My library 1 | Built-in " \
  "the four groups with their counts"
ui_expect_new "Processes row: process/mine Mine (design), the design's" "This design: Mine, the design's process"
ui_expect_new "Processes row: process/draft Draft (project)" "This project: Draft"
ui_expect_new "Processes row: process/bench Bench (user)" "My library: Bench"
ui_focus_dialog '^Printers$'
ui_focus_dialog '^Processes$'
echo "ok   the Printers and the Processes managers are two windows, open at once"
ui_expect_log "Processes search at" "the search above the list"
ui_mark
ui_step "search 'bench'"                   ui_click_logged "Processes search"
xdotool type --delay 40 "bench"
ui_expect_new "Processes search: bench, " "the search runs across the groups"
ui_expect_new "Processes groups: This design 0 | This project 0 | My library 1 | Built-in " \
  "only My library's Bench matches, the headings count the matches"
ui_step "clear the search"                 ui_key ctrl+a BackSpace
ui_expect_new "Processes search: , " "the search cleared"

echo "--- 2. This project's Draft: edited in the project's library, the other design takes it"
ui_expect_log "Processes project item Draft at" "Draft has its place"
ui_mark
ui_step "Draft"                            ui_click_logged "Processes project item Draft"
ui_expect_new "Profile selected: process Draft (project)" "Draft is selected"
ui_expect_new "Processes editing: Editing: this project's library — every design of the project uses it" \
  "the editor says where it is edited and whom it reaches"
ui_expect_new "Processes name at" "its name has its place"
ui_mark
ui_step "its name"                         ui_click_logged "Processes name"
ui_key ctrl+a
xdotool type --delay 40 "Draft Fine"
ui_key Return
ui_expect_new "Profile Draft (project) name = Draft Fine" "renamed where it lives"
expect_item "$PROJECT_LIB/print/processes/draft.json" '.profile.name' '"Draft Fine"' \
  "the project's library has the new name"
title=$(xdotool getwindowname "$UI_WINDOW")
echo "     the design's title: $title"
# The other design of the project: its copy of Draft was changed
# elsewhere, and takes the change.
cat > "$WORK/check.json" << EOF
[
  {"expect": {"query": {"query": "library_items", "kind": "process", "project": "$PROJ", "by_place": true},
              "result": {"items": [{"place": "design", "id": "draft", "name": "Draft", "changed": true},
                                   {"place": "project", "id": "draft", "name": "Draft Fine"},
                                   {"place": "user", "id": "bench", "name": "Bench"}]}}},
  {"cmd": "update_from_print_library", "kind": "process", "id": "draft", "project": "$PROJ"},
  {"expect": {"query": {"query": "print_profiles"}, "result": [{"id": "draft", "name": "Draft Fine"}]}}
]
EOF
cli "$WORK/check.json" --open "$PROJ/other.mitcad"
echo "ok   the other design of the project sees Draft Fine and takes it"

echo "--- 3. Copy to: This design's Mine into This project"
ui_focus_dialog '^Processes$'
ui_mark
ui_step "Mine"                             ui_click_logged "Processes design item Mine"
ui_expect_new "Profile selected: process Mine (design)" "Mine is selected"
ui_expect_new "Processes editing: Editing: this design only" "edited in this design only"
ui_expect_new "Processes copy_to at" "Copy to has its place"
ui_mark
ui_step "Copy to"                          ui_click_logged "Processes copy_to"
ui_expect_new "Processes copy_to menu: This project | My library" "the other places"
ui_key Down Return
ui_expect_new "Profile copied to project: Mine" "copied into the project's library"
expect_item "$PROJECT_LIB/print/processes/mine.json" '.profile.name' '"Mine"' "its file in the project's library"
ui_expect_new "Processes row: process/mine Mine (project)" "listed under This project"

echo "--- 3b. Import from Slicer into the place chosen: This project"
ui_expect_log "Processes import_into at" "the box that says where the import goes"
ui_mark
ui_step "Import into: This project"        ui_choose "Processes import_into" 1
ui_expect_new "Processes import into: This project" "This project chosen"
ui_mark
ui_step "Import from Slicer"               ui_click_logged "Processes import"
ui_expect_new "Processes import menu: From a file… | Presets of Bambu Studio…" "a file or the slicer's presets"
ui_key Down Down Return
ui_expect_new "Slicer presets row: process Fine, ticked" "the slicer's process, ticked"
ui_focus_dialog '^Import Presets$'
ui_step "Import (Alt+I)"                   ui_key alt+i
ui_expect_new "Processes import: Fine imported into This project" "imported into the project's library"
ui_expect_new "Profile selected: process Fine (project)" "shown under This project, selected"
expect_item "$PROJECT_LIB/print/processes/fine.json" '.profile.values.layer_height' '0.12' \
  "its file in the project's library"
ui_focus_dialog '^Processes$'
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Processes closed" "closed"
ui_focus_dialog '^Printers$'
ui_step "close (Esc)"                      ui_key Escape

echo "--- 4. Materials: a built-in one is read only, its Duplicate asks where"
ui_focus_main
ui_mark
ui_command "Edit Materials"
ui_expect_new "Materials opened" "the Materials manager opened"
ui_expect_new "Materials groups: This design 0 | This project 0 | My library 0 | Built-in " "the four groups"
ui_expect_new "Materials search at" "the search above the list"
ui_focus_dialog '^Materials$'
ui_mark
ui_step "search 'PLA Plastic'"             ui_click_logged "Materials search"
xdotool type --delay 40 "PLA Plastic"
ui_expect_new "Materials search: PLA Plastic, " "the list shows the matches"
ui_expect_new "Materials mitcad item PLA Plastic at" "Built-in's PLA has its place"
ui_mark
ui_step "PLA Plastic"                      ui_click_logged "Materials mitcad item PLA Plastic"
ui_expect_new "Material selected: PLA Plastic (mitcad)" "PLA is selected"
ui_expect_new "Materials editing: Built-in: read only. Duplicate makes a copy you can edit." \
  "read only, the way on named"
ui_expect_new "Materials header_duplicate at" "Duplicate beside Built-in"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Materials copy_to at"; then
  ui_fail "a built-in material offers Copy to"
fi
ui_mark
ui_step "Duplicate"                        ui_click_logged "Materials header_duplicate"
ui_expect_new "Materials duplicate menu: This design | This project | My library" \
  "Duplicate asks where, This design first"
ui_key Down Down Down Return
ui_expect_new "Material duplicated: PLA Plastic as " "a copy made"
id=$(logged_after "Material duplicated: PLA Plastic as " 's/.* as \(.*\) in \(.*\)$/\1/')
place=$(logged_after "Material duplicated: PLA Plastic as " 's/.* as .* in \(.*\)$/\1/')
[ "$place" = user ] || ui_fail "the copy went to '$place', not My library"
expect_item "$LIB/materials/$id.json" '.material.name' '"PLA Plastic (copy)"' "its file in My library"
ui_expect_new "Materials editing: Editing: My library — every design of yours can use it" \
  "the copy is edited in My library"
ui_mark
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Materials closed" "closed"

echo "--- 5. Tools: no My Library…"
ui_focus_main
ui_mark
ui_step "the Tools menu (Alt+T)"           ui_key alt+t
ui_expect_new "Tools menu: " "the Tools menu opened"
tools=$(logged_after "Tools menu: " 's/.*Tools menu: //')
case "$tools" in
  *"My Library"*) ui_fail "the Tools menu still has My Library: $tools" ;;
esac
echo "ok   the Tools menu has no My Library: $tools"
ui_step "close it (Escape)"                ui_key Escape

ui_finish "UI print managers"
