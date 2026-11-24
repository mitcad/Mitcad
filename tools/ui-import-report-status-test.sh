#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/framework/NoticeLevel.cpp app/framework/TestDriver.cpp
# check-all sources: core/import-base/src/report_status.rs core/import-replay/src/report.rs core/ffi/src/lib.rs
# check-all sources: core/ffi/src/ipt_import.rs core/ffi/src/iam_import.rs core/ipt core/tests
# check-all sources: tools/ui-scripts/import-report-status.mitcad-ui tools/ui-scripts/import-report-status-iam.mitcad-ui
# check-all sources: tools/ui-scripts/import-report-fits.mitcad-ui app/files/F3dImport.cpp app/files/ImportItemProgress.cpp
# The Import Report's status on every row and its one-line verdict
# (mitcad#1893) through the real UI, the shared input scripts
# tools/ui-scripts/import-report-status.mitcad-ui and
# import-report-status-iam.mitcad-ui (also run by
# tools/ui-windows-import-report-status-test.ps1 and
# tools/ui-macos-import-report-status-test.sh), on the in-tree test files
# test_exchange writes:
#   1. --open of designed.ipt (--write-ipt-design: a sketch and an
#      extrusion making the cube the file stores): the verdict says
#      everything came in, every row is OK, no warnings; the filter shows
#      no error rows, then all three again;
#   2. --open of top.iam (--write-iam), one of whose files is missing: the
#      verdict says one file could not be imported, that file's row is an
#      error; the filter shows it alone; the progress's number never
#      goes back (mitcad#2331);
#   3. designed.ipt again with 150 test warnings
#      (MITCAD_TEST_IMPORT_REPORT_WARNINGS, import-report-fits.mitcad-ui):
#      the report stays within the screen and scrolls (mitcad#2331).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-import-report-status-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
test_exchange=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-import-report-status.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

# wait_driver what: waits for the driver's run of the application to end.
wait_driver() {
  for _ in $(seq 1 2000); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed"; exit 1; }
}

"$test_exchange" --write-ipt-design "$work/designed.ipt" > "$work/write.log" 2>&1 ||
  { cat "$work/write.log"; ui_fail "test_exchange: the part with a design"; exit 1; }
"$test_exchange" --write-iam "$work/iam" >> "$work/write.log" 2>&1 ||
  { cat "$work/write.log"; ui_fail "test_exchange: the test assemblies"; exit 1; }
ui_start_display

MITCAD_TEST_INPUT=$here/ui-scripts/import-report-status.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/designed.ipt"
wait_driver 'the part'
ui_expect_log 'TestDriver: done' 'designed.ipt: everything came in, every row OK, the filter'
grep -E '^Import report (verdict|row): designed\.ipt: ' "$UI_LOG" | sed 's/^/     /'

MITCAD_TEST_INPUT=$here/ui-scripts/import-report-status-iam.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/iam/top.iam"
wait_driver 'the assembly'
ui_expect_log 'TestDriver: done' 'top.iam: one file could not be imported, its row an error, the filter'
grep -E '^Import report (verdict|row): top\.iam: ' "$UI_LOG" | sed 's/^/     /'
# The progress's number never goes back (mitcad#2331).
grep -E '^Import progress: ' "$UI_LOG" | sed 's/^/     /'
if grep -E '^Import progress: ' "$UI_LOG" | awk '{ n = $(NF - 2) + 0; if (n < last) bad = 1; last = n } END { exit bad }'; then
  echo "ok   top.iam: the progress's number never goes back"
else
  ui_fail "top.iam: the progress's number went back"
fi

MITCAD_TEST_INPUT=$here/ui-scripts/import-report-fits.mitcad-ui MITCAD_TEST_IMPORT_REPORT_WARNINGS=150 \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/designed.ipt"
wait_driver 'the long report'
ui_expect_log 'TestDriver: done' 'designed.ipt with 150 warnings: within the screen, it scrolls, the filter'
grep -E '^Import report size: designed\.ipt: ' "$UI_LOG" | sed 's/^/     /'
ui_finish 'UI the Import Report: a status per row and a verdict'
