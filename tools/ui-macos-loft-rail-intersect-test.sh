#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: geometry/src/loft_skin.cpp geometry/src/loft.cpp core/model/src/features/loft.rs tools/cli
# check-all sources: tools/ui-scripts/loft-rail-intersect.mitcad-ui tools/ui-scripts/loft-rail-intersect.json
# A loft from a circle to a square through a rail tied to Intersect points,
# and a loose rail's miss in words (mitcad#1944): the shared input script
# tools/ui-scripts/loft-rail-intersect.mitcad-ui of
# tools/ui-loft-rail-intersect-test.sh in the Qt frontend.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-loft-rail-intersect-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/loft-rail-intersect.json" --save "$MITCAD_TEST_WORK/loft.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/loft-rail-intersect.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app loft-rail-intersect --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/loft.mitcad"
ui_expect_log '^Added loft [(]New Body[)] through 2 sections' 'the loft through the rail tied to Intersect points builds'
ui_expect_log '^Panel Loft error: .*rail 1 misses the second profile by 0[.]3 mm' 'the loose rail is refused in words'
ui_expect_log '^TestDriver: done' 'the miss message paints its places, without names or coordinates'
ui_expect_no_log 'TestDriver: FAIL' 'no failed loft rail assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt loft through a rail tied to Intersect points, a loose rail refused in words'
