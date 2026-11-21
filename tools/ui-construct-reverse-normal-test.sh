#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/construction tools/cli
# check-all sources: tools/ui-scripts/construct-reverse-normal.mitcad-ui tools/ui-scripts/construct-reverse-normal.json
# Offset Plane's Reverse Normal and signed Distance (mitcad#2048, GitHub
# mitcad/Mitcad#443) through the real UI, the shared input script
# tools/ui-scripts/construct-reverse-normal.mitcad-ui (also run by
# tools/ui-windows-construct-reverse-normal-test.ps1 and
# tools/ui-macos-construct-reverse-normal-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/construct-reverse-normal.json): the
# Distance tooltip says how to reach the other side, the plane's flip input
# is Reverse Normal, toggling it turns the preview's normal marker over and
# leaves the plane where it is, and Distance -10 mm puts the plane on the
# other side of the face.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-construct-reverse-normal-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-construct-reverse-normal.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/construct-reverse-normal.mitcad-ui
"$cli" run "$here/ui-scripts/construct-reverse-normal.json" --save "$work/construct-reverse-normal.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/construct-reverse-normal.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'reverse normal driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview marker direction Offset Plane normal: 0.000,0.000,1.000' 'the normal marker points up'
ui_expect_log 'Offset Plane: Reverse Normal = on' 'the plane'"'"'s flip input is Reverse Normal'
ui_expect_log 'Preview marker direction Offset Plane normal: 0.000,0.000,-1.000' 'Reverse Normal turned the marker over'
ui_expect_log 'Added plane Plane' 'OK added the plane'
ui_expect_log 'TestDriver: done' 'Reverse Normal and a negative distance'
grep -Eq '^Added plane Plane[0-9]+: origin [(][-0-9.e]+, [-0-9.e]+, 20[)], normal [(]0, 0, -1[)]' "$UI_LOG" ||
  ui_fail 'Distance -10 mm did not put the plane on the other side of the face'
echo "ok   Distance -10 mm put the plane on the other side of the face"
ui_finish 'UI Offset Plane Reverse Normal'
