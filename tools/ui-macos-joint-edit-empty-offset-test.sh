#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/MainWindowJoints.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document.rs tools/cli tools/ui-scripts/joint-edit-empty-offset.mitcad-ui
# A joint edited with its Offset emptied while typing (mitcad#1860) on
# macOS, through the test driver (tools/ui-scripts/joint-edit-empty-offset.mitcad-ui,
# the same script as the Linux and Windows tests'): the edited joint's
# preview keeps the joint and the rigid group after it, an emptied Offset
# moves nothing and OK is refused, a new value moves the group's members
# together and OK commits it.
# Usage: tools/ui-macos-joint-edit-empty-offset-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-joint-edit-empty-offset}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/joint-edit-empty-offset.json" --save "$UI_OUT/files/joint-edit-empty-offset.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/joint-edit-empty-offset.mitcad-ui \
  ui_run_app joint-edit-empty-offset --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/joint-edit-empty-offset.mitcad"
ui_expect_log 'Preview Joint: the last good preview kept' 'the emptied Offset leaves the last good preview drawn'
ui_expect_log 'Joint: OK refused' 'OK refused while the Offset is empty'
ui_expect_log 'TestDriver: done' 'a joint edited with its Offset emptied'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt joint-edit-empty-offset'
