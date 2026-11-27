#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework app/view app/OcctViewer.cpp core/dwg core/ffi/src/dwg_import.rs
# A drawing's import never holds the window (mitcad#589), on the reader's
# test drawing (mitcad-cli import-dwg --write-test):
#   - the import process reads the drawing behind its progress dialog, each
#     item made slower (MITCAD_TEST_RECOMPUTE_DELAY_MS=import_dwg=<ms>):
#     the window answers, and Cancel (Esc) ends the import, the open
#     document as it was;
#   - the window reads the imported design as a job behind its progress
#     dialog, each feature made slower (truss.dwg=<ms>): the window
#     answers, and Esc cancels it, the open document as it was;
#   - once more to its end: the drawing's bodies come in and are shown.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-dwg-import-test.sh
# check-all sources: core/model-base/src/monitor.rs tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-dwg-import.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

"$CLI" import-dwg --write-test "$WORK/truss.dwg" --version R2018 > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "cannot write the test drawing"; }

# open_drawing: File > Open of the drawing (Ctrl+O), its path typed.
open_drawing() {
  ui_key ctrl+o
  ui_type_path 'Open' "$WORK/truss.dwg"
}

# answers "description": the window answers the sync key within 2 s.
answers() {
  local before
  before=$(grep -c '^Sync [0-9]*$' "$UI_LOG")
  xdotool key Pause
  for _ in $(seq 1 100); do
    [ "$(grep -c '^Sync [0-9]*$' "$UI_LOG")" -gt "$before" ] && { echo "ok   $1"; return; }
    sleep 0.02
  done
  ui_fail "$1: the window did not answer within 2 s"
}

expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(ui_window_title)
    [ "$title" = "$1" ] && { echo "ok   title '$1'"; return; }
    sleep 0.2
  done
  ui_fail "window title is '$title', expected '$1'"
}

# unchanged: nothing of the import came into the open document.
unchanged() {
  sleep 1
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Imported truss.dwg|Import report|Bodies shown: F" &&
    ui_fail "the cancelled import changed the document"
  expect_title "Untitled - MitCad"
  echo "ok   the open document stays as it was"
}

ui_start_display
# Each of the drawing's items takes 2 s longer in the import process, each
# feature 1.5 s longer when the window reads the imported design.
export MITCAD_TEST_RECOMPUTE_DELAY_MS=import_dwg=2000,truss.dwg=1500
ui_start_app --no-native-dialogs
unset MITCAD_TEST_RECOMPUTE_DELAY_MS
expect_title "Untitled - MitCad"

echo "--- The import process, cancelled"
ui_mark
ui_step "open the drawing (Ctrl+O)"        open_drawing
ui_expect_new "Import of truss.dwg started" "the import runs in its own process"
ui_focus_dialog '^Import$'
answers "the window answers while the drawing is read"
sleep 2
answers "the window still answers"
ui_key Escape # not a step: the dialog goes
ui_expect_new "Import of truss.dwg cancelled" "Esc cancelled the import"
ui_focus_main
unchanged

echo "--- Reading the imported design, cancelled"
ui_mark
ui_step "open the drawing again (Ctrl+O)"  open_drawing
ui_expect_new "Import of truss.dwg started" "the import started again"
ui_expect_new "Progress dialog shown: truss.dwg" "the window reads the imported design as a job" 120
ui_focus_dialog '^Computing$'
answers "the window answers while it reads the imported design"
ui_key Escape # not a step: it waits for the job
ui_expect_new "Computing truss.dwg: cancel requested" "Esc asks the job to stop"
ui_expect_new "Opening the imported truss.dwg cancelled" "the reading was cancelled"
ui_focus_main
unchanged

echo "--- To its end"
ui_mark
ui_step "open the drawing once more (Ctrl+O)" open_drawing
ui_expect_new "Progress dialog shown: truss.dwg" "the progress dialog" 120
answers "the window answers"
ui_expect_new "Import report: truss.dwg: " "the import's report" 120
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported truss.dwg: 6 bodies" "the drawing's bodies came in"
ui_expect_new "Bodies shown: F" "and are shown"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "faces that could not be meshed" &&
  ui_fail "a face of the test drawing could not be meshed"
expect_title "truss* - MitCad"

ui_finish "UI DWG import test"
