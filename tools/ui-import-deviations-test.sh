#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files core/import-replay/src/deviations.rs core/import-replay/src/report.rs
# check-all sources: core/import-replay/src/lib.rs core/ffi/src/f3d_import.rs core/ffi/src/kernel/exchange.rs
# check-all sources: core/asm/src/testdata.rs core/tests/test_exchange.cpp
# Results that deviate from the file's, kept at the user's choice
# (mitcad#379): an item of an .f3d design whose decoded definition builds a
# result off the bodies stored in the file comes in as the file's stored
# geometry; once the import report is closed, File > Open asks in one
# dialog, a row per such item with how far off it is, which to keep
# parametric. A row selects the faces where it deviates. Kept, the import
# runs again with it parametric, and the report says so.
#   - A synthetic design (test_exchange --write-f3d-deviation): a cube whose
#     history bevels an edge, and its timeline as a dump with a rounding of
#     that edge (MITCAD_TEST_IMPORT_DUMP), so the rounding deviates from
#     the file's bevel.
#   - Use Stored for All (Alt+U): the design stays as imported.
#   - Opened again, the row's Keep Parametric checked (Space) and Apply
#     (Enter): the import runs again with the rounding kept; its report
#     lists it as kept, and no dialog follows.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-import-deviations-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

TEST_EXCHANGE=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
WORK=$(mktemp -d /tmp/mitcad-ui-import-deviations.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

"$TEST_EXCHANGE" --write-f3d-deviation "$WORK" > "$WORK/write.log" 2>&1 || { cat "$WORK/write.log"; ui_fail "no .f3d file"; }
PART="$WORK/Rounding.f3d"
NAME=$(basename "$PART")

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

# close_report: the import report closed.
close_report() {
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"        ui_key Return
}

# the_question: the dialog about the deviating rounding, its row shown.
the_question() {
  ui_expect_new "Import deviations: $NAME: 1 items" "the dialog lists the deviating item"
  ui_expect_new "Import deviation: $NAME: 2 Fillet1 (FilletFeature): volume change -8.584 mm3 against the file's -20.000 (+57.1 %)" \
    "the row says how far off the rounding is"
  ui_expect_new "Import deviation shown: $NAME: Fillet1, 1 faces" "the row is selected"
  ui_expect_new "Selected: 1 face" "the face where it deviates is selected in the view"
  ui_focus_dialog '^Keep Deviating Results\?$'
}

ui_start_display
export MITCAD_TEST_IMPORT_DUMP="$WORK/Rounding.dump.json"
ui_start_app --no-native-dialogs
unset MITCAD_TEST_IMPORT_DUMP

echo "--- the stored geometry for all"
ui_mark
ui_step "open the design (Ctrl+O)"          open_design "$PART"
ui_expect_new "Import of $NAME started" "the import runs in its own process"
ui_expect_new "Import report: $NAME: 3 items (2 parametric, 0 partial, 1 fallback, 0 skipped)" \
  "the rounding came in as the file's geometry" 300
ui_expect_new "Import report deviation: $NAME: Fillet1: the file's stored geometry; volume change -8.584 mm³ against the file's -20.000" \
  "the report lists the deviating result"
close_report
the_question
ui_step "use the stored geometry for all (Alt+U)" ui_key alt+u
ui_expect_new "Import deviations answer: $NAME: stored for all" "nothing is kept"
ui_focus_main
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "runs again keeping"; then
  ui_fail "the import ran again although nothing was kept"
fi
echo "ok   the design stays as imported"

echo "--- the rounding kept parametric"
ui_mark
ui_step "open the design again (Ctrl+O)"    open_design "$PART" discard
ui_expect_new "Import report: $NAME: 3 items (2 parametric, 0 partial, 1 fallback, 0 skipped)" "imported as before" 300
close_report
the_question
ui_step "keep the rounding (Space)"         ui_key space
ui_expect_new "Import deviation choice: $NAME: Fillet1 keep" "the row's choice is keep"
ui_step "apply (Enter)"                     ui_key Return
ui_expect_new "Import deviations answer: $NAME: keep Fillet1 (2)" "the rounding is kept"
ui_expect_new "Import of $NAME runs again keeping 1 items parametric" "the import runs again"
ui_mark
ui_expect_new "Import report: $NAME: 3 items (3 parametric, 0 partial, 0 fallback, 0 skipped)" \
  "the rounding is parametric" 300
ui_expect_new "Import report deviation: $NAME: Fillet1: kept parametric; volume change -8.584 mm³ against the file's -20.000" \
  "the report lists the rounding as kept with its deviation"
close_report
ui_focus_main
ui_sync
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Import deviations: $NAME:"; then
  ui_fail "the dialog asked again although nothing deviates any more"
fi
echo "ok   no dialog after the rounding was kept"

ui_finish "import deviations"
