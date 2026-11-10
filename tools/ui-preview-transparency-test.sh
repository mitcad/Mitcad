#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp
# check-all sources: app/commands/PrimitiveCommands.cpp tools/ui-scripts/preview-transparency.mitcad-ui
# A new sphere's see-through preview without a pattern of triangles
# (mitcad#1575, GitHub mitcad/Mitcad#219): in an empty design, Sphere placed
# on the XY plane; its preview is drawn with its back faces culled (a
# closed solid), and the middle of the preview in the view's frame has no
# steps in brightness from five view directions (expect-view-smooth). The
# shared steps are tools/ui-scripts/preview-transparency.mitcad-ui; the
# Windows twin is tools/ui-windows-preview-transparency-test.ps1, the macOS
# one tools/ui-macos-preview-transparency-test.sh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preview-transparency-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-preview-transparency.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/preview-transparency.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'preview transparency driver timed out'; exit 1; }
wait "$UI_RUNNER" || { grep -E 'TestDriver|View new body' "$UI_LOG" | tail -12 >&2; exit 1; }
grep -E '^View new body |^TestDriver: view (smooth|stepped): ' "$UI_LOG" | sed 's/^/     /'
ui_expect_log 'TestDriver: done' 'a sphere preview see-through without a pattern of triangles'
ui_finish 'UI preview transparency'
