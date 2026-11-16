#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/Dialogs.cpp app/framework/TestDriver.cpp app/WindowManager.cpp
# check-all sources: core/model-api/src/lib.rs core/ffi/src/ipt_import.rs core/ipt core/tests tools/cli
# check-all sources: tools/ui-scripts/import-into-design.mitcad-ui tools/ui-scripts/import-into-design.json
# Design files of other formats into the open design as components
# (mitcad#1894) on macOS in Qt, as tools/ui-import-into-design-test.sh on
# Linux: the shared input script tools/ui-scripts/import-into-design.mitcad-ui
# on a design with content (tools/ui-scripts/import-into-design.json, a
# 20 mm cube) and the two .ipt test parts of test_exchange (--write-ipt,
# --write-ipt-design): File > Import of both at once into this design (one
# Where to Import, each a component named after its file with its history,
# side by side, one undo step, each file's report), Undo taking both out,
# and a third import as a new design in a tab of its own.
# Usage: tools/ui-macos-import-into-design-test.sh <app> <mitcad-cli> <test_exchange> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 4 ]; then
  echo 'usage: ui-macos-import-into-design-test.sh <app> <cli> <test_exchange> <output>' >&2
  exit 2
fi
ui_init "$1" "$4"
cli=$2
test_exchange=$3
UI_TIMEOUT=420
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/import-into-design.json" --save "$MITCAD_TEST_WORK/assembly.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log" >&2; echo 'FAIL: mitcad-cli: the design' >&2; exit 1; }
"$test_exchange" --write-ipt "$MITCAD_TEST_WORK/part.ipt" >> "$UI_OUT/setup.log" 2>&1 &&
  "$test_exchange" --write-ipt-design "$MITCAD_TEST_WORK/designed.ipt" >> "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log" >&2; echo 'FAIL: test_exchange: the test parts' >&2; exit 1; }

MITCAD_TEST_INPUT=$here/ui-scripts/import-into-design.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app import-into-design --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$MITCAD_TEST_WORK/assembly.mitcad"
ui_expect_log '^TestDriver: done' \
  'two .ipt parts imported into the design as components in one undo step; a third as a new design'
ui_expect_no_log 'TestDriver: FAIL' 'no failed import step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt import into this design'
