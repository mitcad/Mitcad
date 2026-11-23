#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/construction core/model/src/features/geom_ref.rs tools/cli
# check-all sources: tools/ui-scripts/construct-axis-on-plane.mitcad-ui tools/ui-scripts/construct-axis-on-plane.json
# Axis Perpendicular at Point on a plane (mitcad#2282) through the real UI,
# the shared input script tools/ui-scripts/construct-axis-on-plane.mitcad-ui
# (also run by tools/ui-windows-construct-axis-on-plane-test.ps1 and
# tools/ui-macos-construct-axis-on-plane-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/construct-axis-on-plane.json): its Face
# field takes a construction plane and the origin plane XY, and the axis
# goes through the point's projection onto the plane, along its normal.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construct-axis-on-plane-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-construct-axis-on-plane.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50 MITCAD_LOG_PICKS=1
export MITCAD_TEST_INPUT=$here/ui-scripts/construct-axis-on-plane.mitcad-ui
"$cli" run "$here/ui-scripts/construct-axis-on-plane.json" --save "$work/axis-on-plane.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli run construct-axis-on-plane.json failed"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/axis-on-plane.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'axis on plane driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added axis Axis1: origin (20, 0, 10), direction (0, 0, 1)' \
  'the axis on a construction plane goes through the point'"'"'s projection along its normal'
ui_expect_log 'Added axis Axis2: origin (20, 0, 0), direction (0, 0, 1)' 'the axis on the origin plane XY'
ui_expect_log 'TestDriver: done' 'both axes made on planes'
ui_finish 'UI construct axis on plane'
