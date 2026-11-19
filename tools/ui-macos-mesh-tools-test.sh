#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Mesh tools for reverse engineering (mitcad#1871) in the Qt frontend on
# macOS: the shared steps tools/ui-scripts/mesh-tools.mitcad-ui (also run by
# tools/ui-mesh-tools-test.sh and tools/ui-windows-mesh-tools-test.ps1) on
# the design mitcad-cli makes (tools/ui-scripts/mesh-tools.json, run in the
# work folder, where it writes its STL file): each panel on a cylinder's
# mesh.
# Usage: bash tools/ui-macos-mesh-tools-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-mesh-tools}"
UI_TIMEOUT=600
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/mesh-tools.mitcad
(cd "$MITCAD_TEST_WORK" && "$2" run "$here/ui-scripts/mesh-tools.json" --save "$design") > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/mesh-tools.mitcad-ui ui_run_app mesh-tools \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log 'TestDriver: done' 'each mesh tool panel made its body'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: Mesh tools; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: the mesh tools panels make their bodies"
