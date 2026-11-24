#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/framework/NoticeLevel.cpp app/framework/TestDriver.cpp
# check-all sources: core/import-base/src/report_status.rs core/import-replay/src/report.rs core/ffi/src/lib.rs core/tests
# check-all sources: tools/ui-scripts/import-report-status.mitcad-ui tools/ui-scripts/import-report-status-iam.mitcad-ui
# check-all sources: tools/ui-scripts/import-report-fits.mitcad-ui app/files/F3dImport.cpp app/files/ImportItemProgress.cpp
# The Import Report's status on every row and its one-line verdict
# (mitcad#1893) on macOS in Qt, as tools/ui-import-report-status-test.sh on
# Linux: the shared input scripts tools/ui-scripts/import-report-status.mitcad-ui
# and import-report-status-iam.mitcad-ui on the files test_exchange writes:
# --open of designed.ipt (--write-ipt-design), whose verdict says
# everything came in, every row OK; --open of top.iam (--write-iam), one of
# whose files is missing: one file could not be imported, its row an
# error; the filter shows the error rows alone, then every row again; the
# progress's number never goes back (mitcad#2331); designed.ipt again with
# 150 test warnings (import-report-fits.mitcad-ui): the report stays within
# the screen and scrolls (mitcad#2331).
# Usage: tools/ui-macos-import-report-status-test.sh <app> <test_exchange> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-import-report-status-test.sh <app> <test_exchange> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
test_exchange=$2
UI_TIMEOUT=480
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
{ "$test_exchange" --write-ipt-design "$MITCAD_TEST_WORK/designed.ipt" &&
  "$test_exchange" --write-iam "$MITCAD_TEST_WORK/iam"; } > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log" >&2; echo 'FAIL: test_exchange: the test files' >&2; exit 1; }

MITCAD_TEST_INPUT=$here/ui-scripts/import-report-status.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app import-report-status --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$MITCAD_TEST_WORK/designed.ipt"
ui_expect_log '^TestDriver: done' 'designed.ipt: everything came in, every row OK, the filter'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step on the part'
[ "$UI_FAILURES" = 0 ] || exit 1

MITCAD_TEST_INPUT=$here/ui-scripts/import-report-status-iam.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app import-report-status-iam --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$MITCAD_TEST_WORK/iam/top.iam"
ui_expect_log '^TestDriver: done' 'top.iam: one file could not be imported, its row an error, the filter'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step on the assembly'
# The progress's number never goes back (mitcad#2331).
if grep -E '^Import progress: ' "$UI_LOG_FILE" | awk '{ n = $(NF - 2) + 0; if (n < last) bad = 1; last = n } END { exit bad }'; then
  ui_pass "top.iam: the progress's number never goes back"
else
  ui_fail "top.iam: the progress's number went back"
fi
[ "$UI_FAILURES" = 0 ] || exit 1

MITCAD_TEST_INPUT=$here/ui-scripts/import-report-fits.mitcad-ui MITCAD_TEST_STEP_MS=100 MITCAD_TEST_IMPORT_REPORT_WARNINGS=150 \
  ui_run_app import-report-fits --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$MITCAD_TEST_WORK/designed.ipt"
ui_expect_log '^TestDriver: done' 'designed.ipt with 150 warnings: within the screen, it scrolls, the filter'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step on the long report'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt the Import Report: a status per row and a verdict'
