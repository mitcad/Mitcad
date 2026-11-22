#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PrimitiveCommands.cpp app/commands/PlaceInputs.cpp
# check-all sources: core/model/src/features/primitive.rs geometry/src/primitive.cpp
# check-all sources: tools/ui-scripts/primitive-cone.mitcad-ui
# Cone (mitcad#2239) in the macOS VM, as tools/ui-primitive-cone-test.sh
# does on Linux: the shared steps tools/ui-scripts/primitive-cone.mitcad-ui
# in an empty design (a cone on XY, made a frustum; volumes and faces
# checked).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-primitive-cone-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=120
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
MITCAD_TEST_INPUT=$here/ui-scripts/primitive-cone.mitcad-ui MITCAD_TEST_STEP_MS=100 MITCAD_LOG_VOLUMES=1 \
  ui_run_app primitive-cone --ui=qt --no-recovery --no-mcp
ui_expect_log '^TestDriver: done' 'a cone placed on XY and made a frustum, its volumes and faces'
ui_expect_no_log 'TestDriver: FAIL' 'no failed primitive cone assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: primitive cone'
