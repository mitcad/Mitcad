#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build core/model/src/document/bom.rs core/model-api/src/libraries.rs tools/cli
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-bom-grouping-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
python3 "$here/ui-bom-grouping-test.py" prepare "$2" "$MITCAD_TEST_WORK" || exit 1
UI_TIMEOUT=180
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/bom.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app bom --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/bom.mitcad"
ui_expect_log '^TestDriver: quit' 'BOM grouped/separate view and actual exports completed'
ui_expect_no_log 'TestDriver: FAIL' 'all BOM UI assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
python3 "$here/ui-bom-grouping-test.py" verify "$MITCAD_TEST_WORK" || exit 1
MITCAD_TEST_INPUT=$MITCAD_TEST_WORK/remembered.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app bom-remembered --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/bom.mitcad"
ui_expect_log '^TestDriver: quit' 'grouped view remembered across application restart'
ui_expect_no_log 'TestDriver: FAIL' 'remembered BOM view assertions passed'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt BOM grouping, shown CSV/PDF and unchanged design'
