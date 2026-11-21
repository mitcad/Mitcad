#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam core/model/src/cam core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-machine-controllers.mitcad-ui tools/ui-scripts/cam-machine-controllers.json
# The Controller list of Machines and Tools… > Edit Machine (mitcad#2119)
# on macOS, through the test driver
# (tools/ui-scripts/cam-machine-controllers.mitcad-ui, the same script as
# the Linux and Windows tests'): the router's list has the milling dialects
# and no lathe dialect, the lathe's only the lathe dialects.
# Usage: tools/ui-macos-cam-machine-controllers-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-machine-controllers}"
UI_TIMEOUT=180
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-machine-controllers.json" --save "$UI_OUT/files/machines.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_OUT/files/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-machine-controllers.mitcad-ui \
  ui_run_app cam-machine-controllers --ui=qt --no-native-dialogs --no-recovery --no-mcp \
  --open "$UI_OUT/files/machines.mitcad"
ui_expect_log 'TestDriver: done' "the router's and the lathe's controller lists"
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
grep -Eq '^CAM machine controllers: router: .*turning' "$UI_LOG_FILE" &&
  ui_fail "the router's Controller list offers a lathe dialect"
[ "$UI_FAILURES" = 0 ] || exit 1
