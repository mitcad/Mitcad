#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/framework/CommandSession.cpp app/commands/ExtrudeCommands.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# An Extrude Join's target body in the command's look (mitcad#2069) on
# macOS in Qt, as tools/ui-extrude-join-tint-test.sh on Linux: the shared
# input script tools/ui-scripts/extrude-join-tint.mitcad-ui on the design
# tools/ui-scripts/extrude-join-tint.json makes: the Join's target tinted
# faintly, nearer its own color than the cut's red, and the added material
# blue.
# Usage: tools/ui-macos-extrude-join-tint-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-extrude-join-tint-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/extrude-join-tint.json" --save "$MITCAD_TEST_WORK/tint.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/extrude-join-tint.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app extrude-join-tint --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/tint.mitcad"
ui_expect_log '^TestDriver: view point pixel .*, off ' 'the target body nearer its own color than the red'
ui_expect_log '^TestDriver: view point pixel .*, blue [0-9]* over red ' 'the added material blue'
ui_expect_log '^TestDriver: done' 'the Join target tint'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt extrude join tint'
