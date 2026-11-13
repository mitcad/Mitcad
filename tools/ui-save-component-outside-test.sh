#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowSaveComponent.cpp app/files/SaveComponentTarget.cpp app/browser
# check-all sources: core/model/src/document/link_copies.rs core/model/src/document/components.rs
# check-all sources: core/model/src/document/save_component.rs core/model-api/src/components.rs tools/cli
# Save Component as Design with Replace with a link to a file outside the
# project (mitcad#1739), through the real UI, on a holder design in a
# project (a folder with .mitcad/project.json, no versions) with the
# components Box:1 and Plate:1 that mitcad-cli makes (synthetic blocks):
#   1. Box:1 with Replace with a link, the file given in a folder beside
#      the project: before saving, the dialog says the file is outside the
#      project, so a link to it is not followed, and offers Save into the
#      Project and Link (the default), Save There and Link and Cancel.
#      Enter saves into the project's parts folder and links that file;
#      nothing is written outside.
#   2. Plate:1 without Replace with a link, outside the project: no
#      dialog, the file written where it was asked.
#   3. Saved, and opened again: the link names the file in the project
#      from the design's folder, and opening says nothing of a link not
#      followed.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-save-component-outside-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-save-component-outside.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
PROJECT=$WORK/project
OUTSIDE=$WORK/elsewhere
HOLDER=$PROJECT/designs/holder.mitcad
mkdir -p "$PROJECT/.mitcad" "$PROJECT/designs" "$OUTSIDE"
printf '{"format": "mitcad-project", "version": 1}\n' > "$PROJECT/.mitcad/project.json"

cat > "$WORK/holder.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Box", "transform": {"translation": [0, 50, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "create_component", "name": "Plate", "transform": {"translation": [50, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 20, "height": 2, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"}
]
EOF
"$CLI" run "$WORK/holder.json" --save "$HOLDER" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (holder)"; }

ENTRY="Save Component as Design…"
# row_menu ROW: the context menu of a browser row ("Browser Root/...").
row_menu() {
  ui_focus_main
  ui_mark
  ui_step "right-click $1"                 ui_click_logged "$1" 3
  ui_expect_new "Context menu: " "the menu of $1 opened"
}

ui_start_display
ui_start_app --open "$HOLDER" --no-native-dialogs
ui_expect_log "Opened $HOLDER" "the holder opened in its project"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/Box:1 at" "the browser lists Box:1"

echo "--- 1. Replace with a link to a file outside the project: said before saving, saved into the project"
row_menu "Browser Root/Box:1"
ui_mark
ui_step "choose Save Component as Design…" ui_menu_choose "$ENTRY"
ui_expect_new "Save Component as Design dialog opened: Box; references: none" "the dialog opened"
ui_focus_dialog '^Save Component as Design$'
ui_mark
ui_step "Replace with a link (Alt+R)"      ui_key alt+r
ui_expect_new "replace with a link checked: yes" "the box is checked"
ui_step "Save… (Enter)"                    ui_key Return
ui_expect_new "Save Component as Design dialog: replace with a link: yes" "replaced"
ui_step "a file beside the project"        ui_type_path "Save Design File" "$OUTSIDE/Box.mitcad"
ui_expect_new "Save Component as Design outside the project: Box.mitcad (project); choices Save into the Project and Link | Save There and Link | Cancel" \
  "the dialog offers the project, the file as asked and Cancel"
ui_expect_new "Save Component as Design outside the project text: Box.mitcad is outside this design's project, so a link to it is not followed: changes to the file will not come in. Save it into the project's parts folder and link that file, or save it where you chose and link it as it is." \
  "the dialog says in words why and what to do"
ui_focus_dialog '^Save Component as Design$'
ui_step "Save into the Project and Link (Enter)" ui_key Return
ui_focus_main
ui_expect_new "Save Component as Design outside the project: save into the project and link" "the project chosen"
ui_expect_new "Save Component as Design: saving into the project as parts/Box.mitcad" "into the project's parts folder"
ui_expect_new "Saved component Box as design $PROJECT/parts/Box.mitcad: 0 reference(s), replaced: yes" \
  "written in the project and replaced by a link" 30
[ -f "$PROJECT/parts/Box.mitcad" ] || ui_fail "no Box.mitcad in the project's parts folder"
[ ! -e "$OUTSIDE/Box.mitcad" ] || ui_fail "Box.mitcad was written outside the project too"
echo "ok   the file is in the project only"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 2. Without Replace with a link: no question, the file where it was asked"
row_menu "Browser Root/Plate:1"
ui_mark
ui_step "choose Save Component as Design…" ui_menu_choose "$ENTRY"
ui_expect_new "Save Component as Design dialog opened: Plate; references: none" "the dialog opened"
ui_focus_dialog '^Save Component as Design$'
ui_mark
ui_step "Save… (Enter)"                    ui_key Return
ui_expect_new "Save Component as Design dialog: replace with a link: no" "not replaced"
ui_step "a file beside the project"        ui_type_path "Save Design File" "$OUTSIDE/Plate.mitcad"
ui_focus_main
ui_expect_new "Saved component Plate as design $OUTSIDE/Plate.mitcad: 0 reference(s), replaced: no" \
  "written where it was asked" 30
grep -qF "Save Component as Design outside the project" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "asked about a link although nothing is linked"
echo "ok   no outside dialog without a link"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 3. Saved and opened again: the link is followed"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved " "saved"
ui_wait_idle
grep -qF '"path": "../parts/Box.mitcad"' "$HOLDER" || ui_fail "the link does not name the file in the project: $(grep -F '"path"' "$HOLDER")"
! grep -qF "$OUTSIDE" "$HOLDER" || ui_fail "the design names a file outside the project"
echo "ok   the link names the file in the project from the design's folder"
ui_stop_app
# A log of its own for the second start (ui_start_app writes it anew).
ui_start_app --open "$HOLDER" --no-native-dialogs
ui_expect_log "Opened $HOLDER" "opened again"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/Box:1 at" "the linked component is listed"
grep -qF "link is not followed" "$UI_LOG" && ui_fail "opening says a link is not followed: $(grep -F 'link is not followed' "$UI_LOG")"
echo "ok   no link not followed"
ui_stop_app

ui_finish "UI save component as design outside the project"
