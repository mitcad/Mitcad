#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchController.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/solver core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui tools/ui-scripts/sketch-perpendicular-arc.json
# Perpendicular of a line and an arc (mitcad#2114) through the real UI on
# a design that mitcad-cli makes (tools/ui-scripts/sketch-perpendicular-arc.json):
# Shift+R, the line, then the arc add one perpendicular; the line turns
# through the arc's center, its symbol shows, and one undo step takes it
# away. The steps are tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui,
# which tools/ui-windows-sketch-perpendicular-arc-test.ps1 runs on Windows
# and tools/ui-macos-sketch-perpendicular-arc-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-perpendicular-arc-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-perpendicular-arc.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$work} MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-perpendicular-arc.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-perpendicular-arc.json" --save "$work/perpendicular-arc.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/perpendicular-arc.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch Perpendicular arc driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added constraint perpendicular k' 'Perpendicular took the line and the arc'
ui_expect_log '(perpendicular by c3)' "the Perpendicular's symbol shows"
ui_expect_log 'TestDriver: done' 'the line turned through the center, one undo step'
ui_finish 'UI sketch Perpendicular of a line and an arc'
