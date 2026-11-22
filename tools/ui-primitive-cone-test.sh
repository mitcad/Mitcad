#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PrimitiveCommands.cpp app/commands/PlaceInputs.cpp
# check-all sources: core/model/src/features/primitive.rs geometry/src/primitive.cpp
# check-all sources: tools/ui-scripts/primitive-cone.mitcad-ui
# Cone (mitcad#2239): in an empty design, Cone placed on the XY plane
# with its defaults is one body of 1000 pi mm3 with two faces; edited into
# a frustum (Top Radius 4 mm) it is 1560 pi mm3 with three faces; Undo and
# Redo go back and forth. The shared steps are
# tools/ui-scripts/primitive-cone.mitcad-ui; the Windows twin is
# tools/ui-windows-primitive-cone-test.ps1, the macOS one
# tools/ui-macos-primitive-cone-test.sh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-primitive-cone-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-primitive-cone.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100 MITCAD_LOG_VOLUMES=1
export MITCAD_TEST_INPUT=$here/ui-scripts/primitive-cone.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'primitive cone driver timed out'; exit 1; }
wait "$UI_RUNNER" || { grep -E 'TestDriver|Cone|Volume|volume' "$UI_LOG" | tail -16 >&2; exit 1; }
grep -E '^(Added Cone|Edited F1)|[Vv]olume' "$UI_LOG" | sed 's/^/     /'
ui_expect_log 'TestDriver: done' 'a cone placed on XY and made a frustum, its volumes and faces'
ui_finish 'UI primitive cone'
