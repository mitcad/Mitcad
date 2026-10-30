#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/report core/ffi/src/f3d_import.rs core/ffi/src/f3d_references.rs core/ffi/src/rvt_import.rs
# check-all sources: core/import-replay core/f3d-design core/rvt core/model-base/src/reader_guard.rs core/tests
# The listings before an import run in the import process, not in the
# application (mitcad#922):
#   1. A synthetic assembly whose documents are in another folder
#      (test_exchange --write-f3d-references): its references are listed
#      by the import process behind the progress dialog, and the file
#      dialogs are asked when the listing returns; both skipped, it imports
#      with the components empty.
#   2. The same with the part's file chosen: listed again in the import
#      process, the other document found next to the part, no second
#      dialog; both documents imported.
#   3. The assembly with its MetaStream.dat replaced by a deliberately
#      corrupt stream, its reader made to panic on it as a parser bug would
#      (MITCAD_TEST_PARSER_PANIC, a test hook): the import fails with "the
#      import process stopped", the crash report is the import process's,
#      and MitCad keeps running and answering. Likewise the design packed
#      in an .f3z, and a building project's link listing (test_exchange
#      --write-rvt).
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-import-listing-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

TEST_EXCHANGE=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-import-listing.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# The crashes of the import process are on purpose: no core files, and
# the app counts as crashed only when it is gone or gdb stopped it.
ulimit -c 0
ui_crashed() { grep -q "^Program received signal" "$UI_LOG" || ! kill -0 "$UI_RUNNER" 2> /dev/null; }
# Their crash reports are kept, not offered.
CRASHES=$XDG_DATA_HOME/Mitcad/Mitcad/crashes
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[reports]\nofferErrors=false\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

# open_file path: File > Open (Ctrl+O), the path typed.
open_file() {
  ui_key ctrl+o
  ui_type_path 'Open' "$1"
}

# expect_alive what: MitCad is still running and answers the sync key.
expect_alive() {
  ui_focus_main
  ui_mark
  ui_sync
  kill -0 "$(ui_app_pid)" 2> /dev/null || ui_fail "MitCad ended after $1"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q '^Sync [0-9]*$' || ui_fail "MitCad does not answer after $1"
  echo "ok   MitCad keeps running and answers after $1"
}

# expect_stopped file kind: with MITCAD_TEST_PARSER_PANIC naming the file,
# its listing (kind: references or links) panics in the import process:
# the import fails as that process stopped, its crash report is the import
# process's, and MitCad goes on.
expect_stopped() {
  local name before
  name=$(basename "$1")
  before=$(ls "$CRASHES" 2> /dev/null | wc -l)
  MITCAD_TEST_PARSER_PANIC=$name ui_start_app --demo --no-native-dialogs
  ui_mark
  ui_step "open $name (Ctrl+O)"            open_file "$1"
  ui_expect_new "Import $2 of $name: listed in the import process" "the $2 listed in the import process"
  ui_expect_new "Import of $name failed: the import process stopped" "the import failed: its process stopped" 60
  ui_focus_dialog '^Import$'
  ui_step "close the error (Enter)"        ui_key Return
  expect_alive "the panic in the listing of $name"
  [ "$(ls "$CRASHES" 2> /dev/null | wc -l)" -gt "$before" ] || ui_fail "no crash report of the import process"
  report=$(grep -l "MITCAD_TEST_PARSER_PANIC" "$CRASHES"/* 2> /dev/null | xargs -r grep -l '^process: import-worker$' |
    head -1)
  [ -n "$report" ] || ui_fail "no crash report of the import process with the panic's message"
  echo "ok   the crash report is the import process's: $(basename "$report")"
  ui_stop_app
}

mkdir -p "$WORK/refs"
"$TEST_EXCHANGE" --write-f3d-references "$WORK/refs" parts || ui_fail "cannot write the test assembly"
ASSEMBLY="$WORK/refs/Assembly v1.f3d"
FIND_PART='Find a Document Assembly v1.f3d Refers To, Version 2'

ui_start_display

echo "--- 1. The references listed in the import process, both skipped"
ui_start_app --demo --no-native-dialogs
ui_mark
ui_step "open the assembly (Ctrl+O)"       open_file "$ASSEMBLY"
ui_expect_new "Import references of Assembly v1.f3d: listed in the import process" \
  "the references listed in the import process"
ui_expect_new "References dialog: ? version 2 of Assembly v1.f3d" "asked for the part when the listing returned" 60
ui_focus_dialog '^Find .*, Version 2$'
ui_step "skip the part (Escape)"           ui_key Escape
ui_expect_new "References dialog: skipped" "the part skipped"
ui_expect_new "References dialog: ? version 1 of Assembly v1.f3d" "asked for the sub-assembly"
ui_focus_dialog '^Find .*, Version 1$'
ui_step "skip the sub-assembly (Escape)"   ui_key Escape
ui_expect_new "Import references of Assembly v1.f3d: 0 found, 2 skipped" "both skipped"
ui_expect_new "Import of Assembly v1.f3d started" "then the import"
ui_expect_new "Imported Assembly v1.f3d: 0 bodies" "imported with the components empty" 120
ui_stop_app

echo "--- 2. The part chosen: listed again, the sub-assembly found next to it"
ui_start_app --demo --no-native-dialogs
ui_mark
ui_step "open the assembly (Ctrl+O)"       open_file "$ASSEMBLY"
ui_expect_new "References dialog: ? version 2 of Assembly v1.f3d" "asked for the part" 60
ui_step "choose the part's file"           ui_type_path "$FIND_PART" "$WORK/refs/parts/Part v2.f3d"
ui_expect_new "References dialog: chose Part v2.f3d" "the part chosen"
ui_expect_new "Import references of Assembly v1.f3d: 2 found, 0 skipped" \
  "the sub-assembly found in the part's folder" 60
listings=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  grep -c "Import references of Assembly v1.f3d: listed in the import process")
[ "$listings" = 2 ] || ui_fail "the references were listed $listings times, not once more after the choice"
echo "ok   listed once more in the import process after the choice"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "References dialog: ? version 1"; then
  ui_fail "asked for the sub-assembly too"
fi
ui_expect_new "Imported Assembly v1.f3d: 2 bodies" "both documents imported" 120
ui_stop_app

echo "--- 3. A reader's panic on a corrupt design stream: the import process stops, MitCad goes on"
# The assembly with its MetaStream.dat a deliberately corrupt stream.
mkdir -p "$WORK/broken"
(cd "$WORK/broken" && cmake -E tar xf "$ASSEMBLY") || ui_fail "cannot unpack the test assembly"
META=$(find "$WORK/broken" -name MetaStream.dat | head -1)
[ -n "$META" ] || ui_fail "the test assembly has no MetaStream.dat"
printf 'a deliberately corrupt design stream\377\376\375\n' > "$META"
(cd "$WORK/broken" && cmake -E tar cf "$WORK/broken.f3d" --format=zip -- *) || ui_fail "cannot pack the corrupt design"
expect_stopped "$WORK/broken.f3d" references

echo "--- The same design in an .f3z package"
(cd "$WORK" && cmake -E tar cf "$WORK/broken.f3z" --format=zip broken.f3d) || ui_fail "cannot pack the .f3z"
expect_stopped "$WORK/broken.f3z" references

echo "--- A building project's links"
"$TEST_EXCHANGE" --write-rvt "$WORK/project.rvt" || ui_fail "cannot write the test project"
expect_stopped "$WORK/project.rvt" links

ui_finish "UI import listing test"
