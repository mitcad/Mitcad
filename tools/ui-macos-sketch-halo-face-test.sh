#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# Where fully constrained curves get their halo (mitcad#2034), through the
# test driver (tools/ui-scripts/sketch-halo-face.mitcad-ui, shared with
# Linux and Windows): the decision read from the "Sketch halos:" log, the
# same as on the other platforms. No contrast check (the core profile
# drew lines 1 px wide before OCCT port patch 1840, mitcad#2063; the
# widths are app.ui-macos-selection-look's).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo 'usage: ui-macos-sketch-halo-face-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/sketch-halo-face.json" --save "$MITCAD_TEST_WORK/halo-face.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-halo-face.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app sketch-halo-face --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/halo-face.mitcad"
ui_expect_log '^TestDriver: done' 'the halos follow the background and the appearances'
ui_expect_no_log 'TestDriver: FAIL' 'no failed step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: sketch halos over a dark face'
