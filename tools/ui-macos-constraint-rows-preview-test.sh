#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/constraint-rows-preview.mitcad-ui
# Assembly Constraint's preview with more than one row (mitcad#1813) on
# macOS, through the test driver (tools/ui-scripts/constraint-rows-preview.mitcad-ui,
# the same script as the Linux and Windows tests'): an incomplete row is
# left out of the preview, which still places the block by row 1; the
# second row complete places it by both rows, drawn once; OK adds the set
# as previewed.
# Usage: tools/ui-macos-constraint-rows-preview-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-constraint-rows-preview}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/constraint-rows-preview.json" --save "$UI_OUT/files/constraint-rows-preview.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/constraint-rows-preview.mitcad-ui \
  ui_run_app constraint-rows-preview --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/constraint-rows-preview.mitcad"
ui_expect_log 'Preview Assembly Constraint without its incomplete rows [(]rows row 2: Select geometry 1[.][)]: ok' \
  'the empty row left out, row 1 still previewed'
ui_expect_log 'Assembly Constraint picks drawn where the preview places them: 2' \
  'the picks on the block where the preview puts it'
ui_expect_log 'TestDriver: done' 'the rows'"'"' preview and OK'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
