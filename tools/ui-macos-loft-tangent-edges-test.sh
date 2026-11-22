#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: geometry/src/loft_merge.cpp geometry/src/loft.cpp core/model/src/features/loft.rs tools/cli
# check-all sources: tools/ui-scripts/loft-tangent-edges.mitcad-ui tools/ui-scripts/loft-tangent-edges.json
# The Loft panel's Tangent Edges (mitcad#2162): the shared input script
# tools/ui-scripts/loft-tangent-edges.mitcad-ui of
# tools/ui-loft-tangent-edges-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-loft-tangent-edges-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/loft-tangent-edges.json" --save "$MITCAD_TEST_WORK/loft.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/loft-tangent-edges.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app loft-tangent-edges --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/loft.mitcad"
ui_expect_log '^Added loft [(]New Body[)] through 2 sections' 'the circle-to-slot loft builds with Tangent Edges Merge'
ui_expect_log '^TestDriver: done' 'the loft has one side face and its caps'
ui_expect_no_log 'TestDriver: FAIL' 'no failed loft tangent edges assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Loft panel Tangent Edges, Merge by default, one side face from a circle to a slot'
