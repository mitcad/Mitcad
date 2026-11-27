#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/platform/macos app/platform/NativeDialogs.hpp app/platform/NativeMacWindow.hpp app/platform/NativeTestInput.hpp app/MainWindowNative.cpp app/framework/TestDriver
# Native Projects keys, row editor and drags (mitcad#816): Return, F2 and
# Delete with real outline focus, New Design/New Folder/Rename in the visible
# row editor (commit, Tab, Escape, a refused name), moves by dragging within a
# project and refused drops, through keyboard, pointer and context-menu entry
# paths in light and dark appearance, then again after reopening the app.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ $# -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
trap 'projects_edit_exit=$?; trap - EXIT; set +e; ui_cleanup; exit "$projects_edit_exit"' EXIT
UI_TIMEOUT=240
# This workflow edits successive designs in the same Projects outline.
# Keep that explicit now that new documents normally open in another window;
# the separate multi-document workflow checks the default window policy.
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '\n[documents]\nopenInCurrentWindow=true\n' >> "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
author='UI Tester <ui@example.invalid>'
long_design=an-unusually-long-synthetic-design-name-kept-in-the-middle-of-its-row
long_folder=a-long-synthetic-folder-name-for-middle-elision
nested="$MITCAD_PROJECTS_DIR/Nested"
other="$MITCAD_PROJECTS_DIR/Other"
{
  "$cli" project create "$nested" --author "$author" --design base.mitcad
  "$cli" project mkdir "$nested" parts
  "$cli" project mkdir "$nested" parts/fasteners
  "$cli" project mkdir "$nested" "$long_folder"
  "$cli" project new-design "$nested" parts/bracket.mitcad
  "$cli" project new-design "$nested" "parts/fasteners/$long_design.mitcad"
  "$cli" project new-design "$nested" "$long_folder/inside.mitcad"
  "$cli" project new-design "$nested" spare.mitcad
  "$cli" project new-design "$nested" doomed.mitcad
  "$cli" project create "$other" --author "$author" --design elsewhere.mitcad
} > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
run_phase() {
  MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-projects-edit-$1.mitcad-ui \
    ui_run_app "$1" --ui=native --no-recovery || true
  ui_expect_log 'TestDriver: done' "$1: native Projects editing completed"
  for problem in 'TestDriver: FAIL' 'Native Mac canvas:' 'OCCT view event failed' \
                 'Native Projects editor: no row' 'did not take the keyboard'; do
    ui_expect_no_log "$problem" "$1: no $problem"
  done
  if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
}
expect_file() {
  if [ -e "$1" ]; then ui_pass "$2"; else ui_fail "$2: $1 is missing"; fi
}
expect_gone() {
  if [ ! -e "$1" ]; then ui_pass "$2"; else ui_fail "$2: $1 is still there"; fi
}
run_phase keys
expect_file "$nested/parts/holder.mitcad" "F2 renamed the design on disk"
expect_gone "$nested/parts/bracket.mitcad" "the old name is gone"
expect_file "$nested/parts/fasteners/Gear plate.mitcad" "New Design made the named design"
expect_file "$nested/parts/an-equally-long-synthetic-subfolder-name" "New Folder made the named folder"
expect_gone "$nested/bad" "the refused folder name made nothing"
expect_gone "$nested/Folder1" "the refused folder left no folder"
expect_file "$nested/parts/spare.mitcad" "the drag moved the design into the folder"
expect_gone "$nested/spare.mitcad" "the dragged design left its old folder"
expect_gone "$other/spare.mitcad" "the drop on another project moved nothing"
expect_gone "$nested/parts/fasteners/parts" "the drop of a folder into itself moved nothing"
expect_file "$other/elsewhere.mitcad" "the other project kept its design"
expect_gone "$nested/doomed.mitcad" "the confirmed Delete removed the design"
expect_file "$nested/parts/fasteners/$long_design.mitcad" "Escape kept the long name"
expect_file "$nested/$long_folder/inside.mitcad" "the long folder kept its design"
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
run_phase reopened
expect_file "$nested/parts/spare.mitcad" "Escape after reopening kept the design's name"
expect_gone "$nested/parts/renamed.mitcad" "the cancelled name made nothing"
if [ "$UI_FAILURES" -gt 0 ]; then exit 1; fi
echo "PASS: native Projects keys, row editor, drags and context actions, also after reopening"
