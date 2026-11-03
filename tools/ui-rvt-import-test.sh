#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files core/rvt core/cfb core/import-rvt core/import-ifc core/import-base core/ffi/src/rvt_import.rs core/tests tools/cli
# The .rvt import through File > Open (mitcad#274):
#   - the reader's test project (test_exchange --write-rvt: two levels, a
#     column of a family type at its transform, a wall, a surface's saved
#     mesh) imported in the import process, its report shown, the elements
#     in one component per level;
#   - its test family (--write-rfa): the metadata only;
#   - the building test project (--write-rvt-building, mitcad#275): its
#     walls and floor as features, its link found next to it;
#   - with MITCAD_RVT_CORPUS (never committed): the corpus's MIT-licensed
#     project (by its content id), its two missing links asked for and
#     skipped, every building element on its level's
#     component.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-rvt-import-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

TEST_EXCHANGE=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-rvt.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# open_file path: File > Open (Ctrl+O), the path typed; open_unsaved path:
# the same, the open design's changes not saved (an imported design is
# unsaved).
open_file() {
  ui_key ctrl+o
  ui_type_path 'Open' "$1"
}
open_unsaved() {
  ui_key ctrl+o
  ui_focus_dialog '^MitCad$'
  ui_key d
  ui_type_path 'Open' "$1"
}

close_report() {
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"       ui_key Return
  ui_focus_main
}

"$TEST_EXCHANGE" --write-rvt "$WORK/project.rvt" || ui_fail "cannot write the test project"
"$TEST_EXCHANGE" --write-rfa "$WORK/family.rfa" || ui_fail "cannot write the test family"

ui_start_display
ui_start_app --demo --no-native-dialogs

echo "--- the test project"
ui_mark
ui_step "open the project (Ctrl+O)"        open_file "$WORK/project.rvt"
ui_expect_new "Import of project.rvt started" "the import runs in its own process"
ui_expect_new "Import report: project.rvt: 1 saved meshes, 1 boxes at a family type's transform, 1 boxes on 2 levels" \
  "the wall and the surface on the lower level, the column on the upper" 120
ui_expect_new "Import report summary: project.rvt (project) Saved by release 2024" "the report names the release"
close_report
ui_expect_new "Imported project.rvt: 3 bodies" "a new document with the elements' bodies"
ui_expect_new ", Ground (2 bodies), Upper (1 bodies)" \
  "one component per level, its elements' bodies in it"

echo "--- the test family"
ui_mark
ui_step "open the family (Ctrl+O)"         open_unsaved "$WORK/family.rfa"
ui_expect_new "Import report summary: family.rfa (family Test Family & Co) Saved by release 2020" \
  "the family's metadata" 120
close_report

echo "--- the building test project with its link"
"$TEST_EXCHANGE" --write-rvt-building "$WORK/building" || ui_fail "cannot write the building project"
ui_mark
ui_step "open the building project (Ctrl+O)" open_unsaved "$WORK/building/building.rvt"
ui_expect_new "Import report: building.rvt: 0 saved meshes, 0 boxes at a family type's transform, 2 boxes on 2 levels; 0 meshes of the geometry the file keeps, 0 reference bodies, 2 boxes hidden; 6 elements as features; 1 of 1 links found" \
  "its walls and floor as features, its link found next to it (no dialog)" 120
close_report
ui_expect_new "Imported building.rvt: " "the elements' bodies"

if [ -n "${MITCAD_RVT_CORPUS:-}" ] && [ -f "$MITCAD_RVT_CORPUS/index.tsv" ]; then
  echo "--- the corpus's MIT-licensed project"
  PROJECT=$(awk -F'\t' '$2 ~ /^c805df44/ { print $1 }' "$MITCAD_RVT_CORPUS/index.tsv")
  if [ -n "$PROJECT" ] && [ -f "$MITCAD_RVT_CORPUS/$PROJECT" ]; then
    NAME=$(basename "$PROJECT")
    ui_mark
    ui_step "open the project (Ctrl+O)"    open_unsaved "$MITCAD_RVT_CORPUS/$PROJECT"
    # Its two linked models are not in the corpus: asked for, skipped.
    for link in 2024_Facad_Lower.rvt 2024_Facad_Upper.rvt; do
      ui_expect_new "Links dialog: $link of $NAME" "the missing link $link is asked for" 120
      ui_focus_dialog '^Find the Linked Model'
      ui_step "skip $link (Escape)"        ui_key Escape
      ui_expect_new "Links dialog: skipped" "the link is skipped"
    done
    ui_expect_new "Import report: $NAME: " "the project's report" 600
    report=$(grep "Import report: $NAME: " "$UI_LOG" | tail -1)
    echo "ok   ${report#*: }"
    close_report
    ui_expect_new "Imported $NAME: " "the elements' bodies" 300
    bodies=$(grep -E "Imported $NAME: [0-9]+ bodies" "$UI_LOG" | tail -1 | sed 's/.*: \([0-9]*\) bodies.*/\1/')
    [ "${bodies:-0}" -ge 854 ] || ui_fail "only ${bodies:-0} bodies of the project's building elements"
    components=$(grep "Imported $NAME: components " "$UI_LOG" | tail -1)
    [ -n "$components" ] || ui_fail "no components of levels"
    levels=$(grep -o "([0-9]* bodies)" <<< "$components" | wc -l)
    [ "$levels" -ge 12 ] || ui_fail "the elements are on $levels components: $components"
    echo "ok   $bodies bodies on $((levels - 1)) level components"
    ui_step "fit (F6)"                       ui_key F6
    ui_capture "$WORK/project.png"
  else
    echo "skip the corpus project: not in MITCAD_RVT_CORPUS"
  fi
else
  echo "skip the corpus project: MITCAD_RVT_CORPUS is not set"
fi

ui_finish "UI rvt import test"
