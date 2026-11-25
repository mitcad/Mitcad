#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/rib.rs geometry/src/rib.cpp tools/cli
# check-all sources: tools/ui-scripts/web-circle.mitcad-ui tools/ui-scripts/web-circle.json
# A Web from a circle (mitcad#2364): the shared input script
# tools/ui-scripts/web-circle.mitcad-ui of tools/ui-web-circle-test.sh in
# the Qt frontend: a ring wall joined to the plate, one body.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-web-circle-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/web-circle.json" --save "$MITCAD_TEST_WORK/web.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/web-circle.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app web-circle --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/web.mitcad"
ui_expect_log '^Added web on 1 curve\(s\), thickness 2' 'the web is added'
ui_expect_log '^TestDriver: done' 'a web from a circle is a ring wall joined to the plate'
ui_expect_no_log 'TestDriver: FAIL' 'no failed web assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt web circle'
