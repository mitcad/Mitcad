#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli core/ffi/src/serp_test.rs
# check-all sources: core/serp core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/serp.mitcad-ui
# .serp files (mitcad#1870) on macOS in Qt, as tools/ui-serp-test.sh on
# Linux: the shared input script tools/ui-scripts/serp.mitcad-ui on the
# import's test file written by mitcad-cli serp-test-file: --open of it
# makes a new document of its five bodies, the block a component placed
# twice; File > Import of it into that design asks where it goes and makes
# a component named after the file; one undo step takes it back.
# Usage: tools/ui-macos-serp-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-serp-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" serp-test-file "$MITCAD_TEST_WORK/sample.serp" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log" >&2; echo 'FAIL: mitcad-cli: the test file' >&2; exit 1; }

MITCAD_TEST_INPUT=$here/ui-scripts/serp.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app serp --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$MITCAD_TEST_WORK/sample.serp"
ui_expect_log '^TestDriver: done' 'File > Open of the .serp: its bodies and the block placed twice; File > Import asks where it goes'
ui_expect_no_log 'TestDriver: FAIL' 'no failed .serp open or import step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt .serp open and import'
