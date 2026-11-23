#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-lathe-machine.mitcad-ui tools/ui-scripts/cam-lathe-machine.json
# A lathe made in Machines and Tools… (mitcad#2230) on macOS, through the
# test driver (tools/ui-scripts/cam-lathe-machine.mitcad-ui, the same script
# as the Linux and Windows tests'): New Machine… > Lathe and its Edit
# Machine…, a turning tool and a parting tool, a lathe setup, Turn Facing
# and Generate, without any lathe in the design beforehand.
# Usage: tools/ui-macos-cam-lathe-machine-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-lathe-machine}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-lathe-machine.json" --save "$UI_OUT/files/cam-lathe-machine.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_ROOT/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-lathe-machine.mitcad-ui \
  ui_run_app cam-lathe-machine --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$UI_OUT/files/cam-lathe-machine.mitcad"
ui_expect_log 'TestDriver: done' 'a lathe made in Machines and Tools, its tools, a lathe setup, Turn Facing, Generate'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
