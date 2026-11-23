#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/CommandSession.cpp app/framework/CommandSession.hpp
# check-all sources: app/framework/Command.hpp app/framework/CommandPanel.cpp app/framework/CommandPresenter.hpp
# check-all sources: app/framework/CommandRegistry.cpp app/framework/TestDriver.cpp app/browser/TimelineWidget.cpp tools/cli
# check-all sources: core/model/src/constraints.rs core/model/src/features/constraint_set.rs
# check-all sources: tools/ui-scripts/constraint-apply.mitcad-ui tools/ui-scripts/constraint-apply.json
# Constraint, one assembly constraint at a time (mitcad#2293), through the
# real UI, the shared input script tools/ui-scripts/constraint-apply.mitcad-ui
# (also run by tools/ui-windows-constraint-apply-test.ps1 and
# tools/ui-macos-constraint-apply-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/constraint-apply.json): an align with an offset, Apply
# (Constraint1 added, the panel open again with its type), an insert, OK
# (Constraint2 added, the panel closed): two constraint features, each its
# own undo step; Constraint1 edited in the Constraint panel; Undo takes the
# edit and the two constraints back one at a time.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-constraint-apply-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-constraint-apply.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/constraint-apply.mitcad-ui
"$cli" run "$here/ui-scripts/constraint-apply.json" --save "$work/constraint-apply.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/constraint-apply.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'constraint apply driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Panel Constraint Apply at ' 'the new constraint'"'"'s panel has Apply'
ui_expect_log 'Added constraint Constraint1: align placed, moved O2' 'Apply added the align'
ui_expect_log 'Applied Constraint (1)' 'the panel stayed open for the next one'
ui_expect_log 'Added constraint Constraint2: insert placed, moved O3' 'OK added the insert'
ui_expect_log 'Editing F5 with Constraint' 'a constraint of one row edited in the Constraint panel'
ui_expect_log 'Edited F5' 'the edit'
ui_expect_log 'TestDriver: done' 'Apply, OK, the edit and the undo steps'
ui_finish 'UI Constraint Apply'
