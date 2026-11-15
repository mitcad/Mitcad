#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/InspectCommands.cpp app/commands/PropertiesReport.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli geometry/src/extrude.cpp
# check-all sources: geometry/src/sweep.cpp geometry/src/taper_events.cpp
# check-all sources: tools/ui-scripts/taper-closes-slot.mitcad-ui tools/ui-scripts/taper-closes-slot.json
# A widening taper that closes a slot (mitcad#1851) through the real UI:
# the shared input script tools/ui-scripts/taper-closes-slot.mitcad-ui
# (also run by tools/ui-windows-taper-closes-slot-test.ps1 and
# tools/ui-macos-taper-closes-slot-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/taper-closes-slot.json: a block with a slot from
# its top): Extrude with Taper Angle 20 typed succeeds, the slot filled
# where its walls meet, and Properties shows the body's volume.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-taper-closes-slot-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-taper-closes-slot.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/taper-closes-slot.mitcad-ui
"$cli" run "$here/ui-scripts/taper-closes-slot.json" --save "$work/taper-closes-slot.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/taper-closes-slot.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'taper closes slot driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the tapered extrusion closing the slot added, its volume in Properties'
ui_finish 'UI taper closes slot'
