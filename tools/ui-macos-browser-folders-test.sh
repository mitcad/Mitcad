#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/occurrence_folders.rs
# check-all sources: core/model/src/document/bodies.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/browser-folders.mitcad-ui tools/ui-scripts/browser-folders-check.json
# Folders in the Browser (mitcad#2103) on macOS, the same shared script as
# the Linux and Windows tests' (tools/ui-scripts/browser-folders.mitcad-ui)
# on the design mitcad-cli makes (tools/ui-scripts/browser-folders.json):
# components and bodies into folders from the row menu and by dragging,
# the folder's eye, Rename, Ungroup and Undo; the saved design checked with
# browser-folders-check.json.
# Usage: tools/ui-macos-browser-folders-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-browser-folders}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/browser-folders.mitcad
"$2" run "$here/ui-scripts/browser-folders.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-folders.mitcad-ui \
  ui_run_app browser-folders --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log 'TestDriver: done' 'browser-folders: the shared steps'
ui_expect_no_log '^TestDriver: FAIL' 'browser-folders: no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
if "$2" run "$here/ui-scripts/browser-folders-check.json" --open "$design" > "$UI_OUT/check.log" 2>&1; then
  ui_pass 'the saved design (browser-folders-check.json)'
else
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the saved design (browser-folders-check.json)'
  exit 1
fi
[ "$UI_FAILURES" = 0 ] || exit 1
