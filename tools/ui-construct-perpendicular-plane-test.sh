#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/MainWindowContextMenu.cpp app/framework/Command.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/features/construction
# check-all sources: core/model/src/features/coordinate_system.rs tools/cli
# check-all sources: tools/ui-scripts/construct-perpendicular-plane.mitcad-ui tools/ui-scripts/construct-perpendicular-plane.json
# Plane Perpendicular to Face at Point (mitcad#2111) through the real UI,
# the shared input script tools/ui-scripts/construct-perpendicular-plane.mitcad-ui
# (also run by tools/ui-windows-construct-perpendicular-plane-test.ps1 and
# tools/ui-macos-construct-perpendicular-plane-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/construct-perpendicular-plane.json): a
# cylinder's side and a sketch point selected, the view's context menu
# offers the plane in Construct; made level through the side's nearest
# point, turned 90 degrees through the cylinder's axis by its Angle, and a
# sketch on it lies in it, upright.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construct-perpendicular-plane-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-construct-perpendicular-plane.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1
export MITCAD_TEST_INPUT=$here/ui-scripts/construct-perpendicular-plane.mitcad-ui
"$cli" run "$here/ui-scripts/construct-perpendicular-plane.json" --save "$work/perpendicular.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli run construct-perpendicular-plane.json failed"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/perpendicular.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'perpendicular plane driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Command Plane Perpendicular to Face at Point started' 'the context menu offers the plane for a face and a point'
ui_expect_log 'Added plane Plane1: origin (10, 0, 10), normal (0, 0, -1)' 'level through the side'"'"'s nearest point'
ui_expect_log 'Edited F3' 'the Angle changed'
ui_expect_log 'Sketch frame in the design: origin (10.000000, 0.000000, 10.000000), normal (0.000000, 1.000000, 0.000000), x axis (-1.000000, 0.000000, 0.000000)' \
  'a sketch on the turned plane lies in it, upright'
ui_expect_log 'TestDriver: done' 'the perpendicular plane made, turned and sketched on'
ui_finish 'UI construct perpendicular plane'
