#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowSaveComponent.cpp app/browser core/model/src/document
# check-all sources: core/model-api/src/components.rs tools/cli app/files/SaveComponentTarget.cpp
# Save Component as Design (mitcad#1057) through the real UI, on a design
# that mitcad-cli makes: A:1 (a 10 mm cube at x = 100), Box:1 (a cube and
# a sketch with a linked projection of A's top face) and part:1 (a design
# inserted linked):
#   1. Box:1's browser row has Save Component as Design… right after
#      Create Drawing…; A:1's has it too; the root's row and the linked
#      part:1's do not.
#   2. Box:1: the dialog lists the projection of A:1, frozen; Save… with
#      the proposed name writes Box.mitcad in the design's folder, without
#      the link to A:1; the design keeps Box's features.
#   3. Box:1 again with Replace with a link to the new file: Box2.mitcad
#      written and Box:1 linked to it; its row then has no Save Component
#      as Design… (a linked component).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-save-component-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-save-component.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/bench.mitcad

cat > "$WORK/part.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 5, "width": 5, "height": 5, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/part.json" --save "$WORK/part.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (part)"; }
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "A", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "create_component", "name": "Box", "transform": {"translation": [0, 50, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.project", "sketch": "F3", "source": "F1:top", "body": "F1.b0",
    "linked": true, "occurrence": "O1"},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "insert_component", "path": "part.mitcad", "base": "$WORK",
    "transform": {"translation": [0, -50, 0]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ENTRY="Save Component as Design…"
# The last "Context menu: ..." line's entries, " | " between them.
menu_line() { grep -F "Context menu: " "$UI_LOG" | tail -1 | sed "s/.*Context menu: //"; }
# has / lacks ENTRY: the context menu has the entry, or not.
has() {
  [[ "| $(menu_line | sed 's/…/.../g') |" == *"| ${1//…/...} |"* ]] || ui_fail "'$1' is not in the context menu: $(menu_line)"
  echo "ok   the context menu has $1"
}
lacks() {
  [[ "| $(menu_line | sed 's/…/.../g') |" != *"| ${1//…/...} |"* ]] || ui_fail "'$1' is in the context menu: $(menu_line)"
  echo "ok   the context menu has no $1"
}

# row_menu ROW: the context menu of a browser row ("Browser Root/...").
row_menu() {
  ui_focus_main
  ui_mark
  ui_step "right-click $1"                 ui_click_logged "$1" 3
  ui_expect_new "Context menu: " "the menu of $1 opened"
}
close_menu() {
  ui_step "Escape (close the menu)"        ui_key Escape
  ui_step "Escape (clear the selection)"   ui_key Escape
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/Box:1 at" "the browser lists Box:1"

echo "--- 1. Only the rows of the design's own components have it"
row_menu "Browser Root/Box:1"
has "$ENTRY"
[[ "$(menu_line | sed 's/…/.../g')" == *"| Create Drawing... | Save Component as Design... |"* ]] ||
  ui_fail "Save Component as Design… does not follow Create Drawing…: $(menu_line)"
echo "ok   it follows Create Drawing…"
close_menu
row_menu "Browser Root/A:1"
has "$ENTRY"
close_menu
row_menu "Browser Root"
lacks "$ENTRY"
close_menu
ui_expect_log "Browser Root/part:1 at" "the browser lists the linked part:1"
row_menu "Browser Root/part:1"
lacks "$ENTRY"
close_menu

echo "--- 2. Box:1 as a design: the projection of A:1 listed and frozen"
row_menu "Browser Root/Box:1"
ui_mark
ui_step "choose Save Component as Design…" ui_menu_choose "$ENTRY"
ui_expect_new "Save Component as Design dialog opened: Box; references: " "the dialog opened"
ui_expect_new "its projection of A:1, frozen at its current geometry" "the projection of A:1 is listed, frozen"
ui_focus_dialog '^Save Component as Design$'
ui_mark
ui_step "Save… (Enter)"                    ui_key Return
ui_expect_new "Save Component as Design dialog: replace with a link: no" "not replaced"
ui_focus_dialog '^Save Design File$'
ui_step "the proposed name (Enter)"        ui_key Return
ui_focus_main
ui_expect_new "Saved component Box as design $WORK/Box.mitcad: 1 reference(s), replaced: no" \
  "written under the component's name in the design's folder" 30
[ -f "$WORK/Box.mitcad" ] || ui_fail "Box.mitcad was not written"
grep -q '"type": "sketch"' "$WORK/Box.mitcad" || ui_fail "Box.mitcad has no sketch"
! grep -q '"source": "O1"' "$WORK/Box.mitcad" || ui_fail "Box.mitcad still links to A:1"
echo "ok   the new design holds the sketch, its projection unlinked"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 3. Replace with a link to the new file"
row_menu "Browser Root/Box:1"
ui_mark
ui_step "choose Save Component as Design…" ui_menu_choose "$ENTRY"
ui_expect_new "Save Component as Design dialog opened: Box; references: " "the dialog opened"
ui_focus_dialog '^Save Component as Design$'
ui_mark
ui_step "Replace with a link (Alt+R)"      ui_key alt+r
ui_step "Save… (Enter)"                    ui_key Return
ui_expect_new "Save Component as Design dialog: replace with a link: yes" "replaced"
ui_step "another file"                     ui_type_path "Save Design File" "$WORK/Box2.mitcad"
ui_focus_main
ui_expect_new "Saved component Box as design $WORK/Box2.mitcad: 1 reference(s), replaced: yes" \
  "written and replaced by a link" 30
[ -f "$WORK/Box2.mitcad" ] || ui_fail "Box2.mitcad was not written"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_expect_log "Browser Root/Box:1 at" "the browser lists Box:1 still"
row_menu "Browser Root/Box:1"
lacks "$ENTRY"
close_menu

ui_finish "UI save component as design"
