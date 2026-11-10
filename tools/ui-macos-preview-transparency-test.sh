#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp
# check-all sources: app/commands/PrimitiveCommands.cpp tools/ui-scripts/preview-transparency.mitcad-ui
# A new sphere's see-through preview without a pattern of triangles
# (mitcad#1575, GitHub mitcad/Mitcad#219) in the macOS VM, as
# tools/ui-preview-transparency-test.sh does on Linux: the shared steps
# tools/ui-scripts/preview-transparency.mitcad-ui in an empty design.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-preview-transparency-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
MITCAD_TEST_INPUT=$here/ui-scripts/preview-transparency.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app preview-transparency --ui=qt --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'a sphere preview see-through without a pattern of triangles'
ui_expect_no_log 'TestDriver: FAIL' 'no failed preview transparency assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: preview transparency'
