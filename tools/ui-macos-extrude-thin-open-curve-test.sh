#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/extrude-thin-open-curve.mitcad-ui tools/ui-scripts/extrude-thin-open-curve.json
# check-all sources: tools/cli
# An open spline clicked in Extrude without changing Type (mitcad#2594) on
# macOS in Qt, as tools/ui-extrude-thin-open-curve-test.sh on Linux: the
# shared input script tools/ui-scripts/extrude-thin-open-curve.mitcad-ui on
# the design tools/ui-scripts/extrude-thin-open-curve.json makes: the click
# on the spline sets Type to Thin Extrude and takes the spline alone, and
# OK adds the wall along it.
# Usage: tools/ui-macos-extrude-thin-open-curve-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-extrude-thin-open-curve-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/open-curve.mitcad
"$cli" run "$here/ui-scripts/extrude-thin-open-curve.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run extrude-thin-open-curve.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/extrude-thin-open-curve.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app extrude-thin-open-curve --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log '^Extrude Profiles: open sketch curve taken, Type set to Thin Extrude' 'the open spline sets Thin Extrude'
ui_expect_log '^Added thin extrude \(New Body\), distance 10' 'the wall along the spline is added'
ui_expect_log '^TestDriver: done' 'an open spline clicked in Extrude makes a thin extrusion'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt open curve in Extrude sets Thin Extrude'
