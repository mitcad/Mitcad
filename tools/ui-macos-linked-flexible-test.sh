#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowJoints.cpp tools/cli
# check-all sources: core/model/src/document/link_tree.rs core/model/src/document/flexible.rs
# check-all sources: tools/ui-scripts/linked-flexible.mitcad-ui tools/ui-scripts/linked-flexible-source.json
# check-all sources: tools/ui-scripts/linked-flexible-holder.json
# A design linked with its components made Flexible (mitcad#2209) on
# macOS, through the test driver (tools/ui-scripts/linked-flexible.mitcad-ui,
# the same script as the Linux and Windows tests'): Flexible is offered on
# the linked sub-assembly's Browser row, and a drag moves its Key along the
# design's own slider for that occurrence alone.
# Usage: tools/ui-macos-linked-flexible-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-linked-flexible}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/linked-flexible-source.json" --save "$UI_OUT/files/linked-flexible-source.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
sed "s|@SOURCE@|$UI_OUT/files/linked-flexible-source.mitcad|" "$here/ui-scripts/linked-flexible-holder.json" \
  > "$UI_OUT/files/linked-flexible-holder.json"
"$2" run "$UI_OUT/files/linked-flexible-holder.json" --save "$UI_OUT/files/holder.mitcad" \
  >> "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/linked-flexible.mitcad-ui \
  ui_run_app linked-flexible --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/holder.mitcad"
ui_expect_log 'TestDriver: done' 'Slider:1 made flexible, its Key dragged along the design'"'"'s slider alone'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
