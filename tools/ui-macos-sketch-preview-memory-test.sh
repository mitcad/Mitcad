#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/view app/OcctViewer app/framework/TestDriver
# The process footprint stays bounded while sketch previews follow the
# pointer (mitcad#2452, mitcad#2485): Center Point Arc and Sketch Dimension
# on a two-arc profile. The footprint figures are printed for the record.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-sketch-preview-memory}"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
MITCAD_TEST_INPUT=$here/ui-scripts/sketch-preview-memory.mitcad-ui \
  ui_run_app floating --ui=qt --chrome=floating --no-recovery --no-mcp
grep -h "TestDriver: footprint" "$UI_LOG_FILE" | sed 's/^/  /'
ui_expect_log 'TestDriver: done' 'arc and dimension previews kept the footprint bounded'
ui_expect_no_log 'TestDriver: FAIL' 'no failed footprint or sketch assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: sketch preview memory'
