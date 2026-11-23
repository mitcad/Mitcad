#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/constraint-apply.mitcad-ui tools/ui-scripts/constraint-apply.json
# Constraint, one assembly constraint at a time (mitcad#2293), on macOS,
# through the test driver (tools/ui-scripts/constraint-apply.mitcad-ui, the
# same script as the Linux and Windows tests'): an align with an offset,
# Apply (the panel open again with its type), an insert, OK: two
# constraint features, each its own undo step; one edited in the
# Constraint panel; Undo takes them back one at a time.
# Usage: tools/ui-macos-constraint-apply-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-constraint-apply}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/constraint-apply.json" --save "$UI_OUT/files/constraint-apply.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/constraint-apply.mitcad-ui \
  ui_run_app constraint-apply --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/constraint-apply.mitcad"
ui_expect_log 'Applied Constraint [(]1[)]' 'Apply kept the panel open for the next one'
ui_expect_log 'Added constraint Constraint2: insert placed, moved O3' 'OK added the insert'
ui_expect_log 'Editing F5 with Constraint' 'a constraint of one row edited in the Constraint panel'
ui_expect_log 'TestDriver: done' 'Apply, OK, the edit and the undo steps'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
