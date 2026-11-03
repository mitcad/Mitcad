#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files core/import-replay/src/retry.rs core/import-replay/src/report.rs
# check-all sources: core/ffi/src/f3d_import.rs core/ffi/src/jobs.rs core/model-base/src/monitor.rs
# The import's question about the bodies it imported (mitcad#283): when an
# item of an .f3d design is tried again on the file's stored bodies before
# it and that would replace imported bodies that match them, File > Open
# asks once whether to keep the imported (parametric) bodies or to replace
# them; the answer goes to the import process and holds for the rest of the
# import, and the import report names it.
#   - Mitcad's own .f3d file of test bodies (test_exchange --write-f3d),
#     imported with MITCAD_TEST_IMPORT_QUESTION, which makes the import
#     ask at its start as the first such retry does: the question dialog
#     comes over the progress; Replace with Stored Bodies (Alt+R) goes to
#     the import, whose report says so; once more answered with Enter
#     (Keep Imported Bodies, the default).
#   - With MITCAD_F3D_UI_RETRY (a real .f3d design, never committed, whose
#     import retries an item on bodies that match the file's): the real
#     question, answered with Replace; the report names the bodies the
#     retry replaced.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-import-retry-test.sh
# check-all sources: core/tests/test_exchange.cpp

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

TEST_EXCHANGE=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
WORK=$(mktemp -d /tmp/mitcad-ui-import-retry.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

PART="$WORK/bodies.f3d"
"$TEST_EXCHANGE" --write-f3d "$PART" > "$WORK/write.log" 2>&1 || { cat "$WORK/write.log"; ui_fail "no .f3d file"; }

# open_design file [discard]: File > Open of the design; with discard, the
# changes of the document open are not saved.
open_design() {
  ui_key ctrl+o
  if [ -n "${2:-}" ]; then
    ui_focus_dialog '^MitCad$'
    ui_key d
  fi
  ui_type_path 'Open' "$1"
}

# answer_question name key answer: the question of the import of `name`
# answered with `key`, which the import takes as `answer`.
answer_question() {
  ui_expect_new "Import of $1 asks: " "the import asks" 300
  ui_focus_dialog '^Keep the Imported Bodies\?$'
  ui_step "answer ($2)"                     ui_key "$2"
  ui_expect_new "Import of $1 answer: $3" "the answer '$3' goes to the import"
}

# close_report: the import report closed.
close_report() {
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"        ui_key Return
  ui_focus_main
}

ui_start_display
export MITCAD_TEST_IMPORT_QUESTION=1
ui_start_app --no-native-dialogs
unset MITCAD_TEST_IMPORT_QUESTION
NAME=$(basename "$PART")

echo "--- the question answered Replace"
ui_mark
ui_step "open the part (Ctrl+O)"           open_design "$PART"
ui_expect_new "Import of $NAME started" "the import runs in its own process"
answer_question "$NAME" alt+r replace
ui_expect_new "Import report: $NAME:" "the import finished with a report" 300
ui_expect_new "Import report retry: $NAME: Asked at the test question: the stored bodies replace" \
  "the report names the answer"
close_report

echo "--- the question answered Keep (Enter, the default)"
ui_mark
ui_step "open the part again (Ctrl+O)"     open_design "$PART" discard
answer_question "$NAME" Return keep
ui_expect_new "Import report: $NAME:" "the import finished with a report" 300
ui_expect_new "Import report retry: $NAME: Asked at the test question: the imported bodies that match" \
  "the report names the answer"
close_report

if [ -n "${MITCAD_F3D_UI_RETRY:-}" ] && [ -f "${MITCAD_F3D_UI_RETRY:-}" ]; then
  echo "--- a design whose import retries an item (MITCAD_F3D_UI_RETRY)"
  ui_stop_app
  ui_start_app --no-native-dialogs
  RETRY=$(basename "$MITCAD_F3D_UI_RETRY")
  ui_mark
  ui_step "open the design (Ctrl+O)"       open_design "$MITCAD_F3D_UI_RETRY"
  answer_question "$RETRY" alt+r replace
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Import of $RETRY asks: " | grep -q "does not give the file's result" ||
    ui_fail "the question does not name the item that failed"
  ui_expect_new "Import report: $RETRY:" "the import finished with a report" 300
  ui_expect_new "Import report retry: $RETRY: Asked at " "the report names where it asked"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Import report retry: $RETRY: " | grep -qE "; replaced [^n]" ||
    ui_fail "the report names no body a retry replaced"
  echo "ok   the report names the bodies the retry replaced"
  close_report
else
  echo "skip a design whose import retries an item: MITCAD_F3D_UI_RETRY is not set"
fi

ui_finish "import retry question"
