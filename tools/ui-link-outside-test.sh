#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFiles.cpp app/files/FileDialogs.cpp app/files/MainWindowLinks.cpp
# check-all sources: core/model/src/document/link_copies.rs core/model/src/document/components.rs
# check-all sources: core/model/src/document/link_updates.rs core/model/src/file/mod.rs
# check-all sources: core/model-api/src/components.rs tools/cli
# Insert Component of a design outside the project (mitcad#1719, GitHub
# mitcad/Mitcad#251), through the real UI, on a holder design in a project
# (a folder with .mitcad/project.json, no versions) and a design beside the
# project that mitcad-cli makes (synthetic blocks):
#   1. Insert Component of the outside file: before inserting, the dialog
#      says the file is outside the project, so a link to it is not
#      followed, and offers Copy into the Project and Link (the default),
#      Insert as a Copy and Cancel. Enter copies the file into the
#      project's parts folder and links the copy; the status says so.
#   2. Insert Component of the copy, now in the project: the Linked or Copy
#      question as before, no outside dialog.
#   3. Saved, and opened again: the link names the copy from the design's
#      folder, and opening says nothing of a link not followed.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-link-outside-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-link-outside.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
PROJECT=$WORK/project
OUTSIDE=$WORK/elsewhere
HOLDER=$PROJECT/designs/holder.mitcad
mkdir -p "$PROJECT/.mitcad" "$PROJECT/designs" "$OUTSIDE"
printf '{"format": "mitcad-project", "version": 1}\n' > "$PROJECT/.mitcad/project.json"

cat > "$WORK/block.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 10, "height": 5, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$OUTSIDE/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (block)"; }
cat > "$WORK/holder.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [50, 0],
    "length": 20, "width": 20, "height": 2, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/holder.json" --save "$HOLDER" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (holder)"; }

ui_start_display
ui_start_app --open "$HOLDER" --no-native-dialogs
ui_expect_log "Opened $HOLDER" "the holder opened in its project"

echo "--- 1. A design outside the project: said before inserting, copied in"
ui_mark
ui_step "Insert Component (search)"        ui_command "Insert Component"
ui_step "the outside design's file"        ui_type_path "Insert Component" "$OUTSIDE/block.mitcad"
ui_expect_new "Insert Component outside the project: block.mitcad (project); choices Copy into the Project and Link | Insert as a Copy | Cancel" \
  "the dialog offers the copy into the project, a copy and Cancel"
ui_expect_new "Insert Component outside the project text: block.mitcad is outside this design's project, so a link to it is not followed: changes to the file will not come in. Copy it into the project's parts folder and link the copy, or insert it as a copy that can be edited here." \
  "the dialog says in words why and what to do"
ui_focus_dialog '^Insert Component$'
ui_step "Copy into the Project and Link (Enter)" ui_key Return
ui_focus_main
ui_expect_new "Insert Component outside the project: copy into the project and link" "the copy into the project chosen"
ui_expect_new "Insert Component: copied block.mitcad into the project as parts/block.mitcad" \
  "copied into the project's parts folder"
ui_expect_new "Inserted component block (linked) from block.mitcad" "the copy linked, named after the file"
ui_expect_new "Status info: block.mitcad is copied into the project and linked" "the status says so"
[ -f "$PROJECT/parts/block.mitcad" ] || ui_fail "no copy in the project's parts folder"
cmp -s "$OUTSIDE/block.mitcad" "$PROJECT/parts/block.mitcad" || ui_fail "the copy differs from the file"
echo "ok   the copy is the file's text"

echo "--- 2. The copy, in the project: the Linked or Copy question as before"
ui_mark
ui_step "Insert Component (search)"        ui_command "Insert Component"
ui_step "the copy's file"                  ui_type_path "Insert Component" "$PROJECT/parts/block.mitcad"
ui_focus_dialog '^Insert Component$'
ui_step "linked (Enter)"                   ui_key Return
ui_focus_main
ui_expect_new "(linked) from block.mitcad" "inserted linked"
grep -qF "Insert Component outside the project" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "a file in the project was taken for one outside it"
echo "ok   no outside dialog for a file in the project"

echo "--- 3. Saved and opened again: the link is followed"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved " "saved"
ui_wait_idle
grep -qF '"path": "../parts/block.mitcad"' "$HOLDER" || ui_fail "the link does not name the copy: $(grep -F '"path"' "$HOLDER")"
! grep -qF "$OUTSIDE" "$HOLDER" || ui_fail "the design still names the outside file"
echo "ok   the links name the copy from the design's folder"
ui_stop_app
# A log of its own for the second start (ui_start_app writes it anew).
ui_start_app --open "$HOLDER" --no-native-dialogs
ui_expect_log "Opened $HOLDER" "opened again"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/block:1 at" "the linked component is listed"
grep -qF "link is not followed" "$UI_LOG" && ui_fail "opening says a link is not followed: $(grep -F 'link is not followed' "$UI_LOG")"
echo "ok   no link not followed"
ui_stop_app

ui_finish "UI links from outside the project"
