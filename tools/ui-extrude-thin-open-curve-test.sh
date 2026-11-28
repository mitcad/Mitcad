#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/extrude-thin-open-curve.mitcad-ui tools/ui-scripts/extrude-thin-open-curve.json
# check-all sources: tools/cli
# An open spline clicked in Extrude without changing Type (mitcad#2594)
# through the real UI, the shared input script
# tools/ui-scripts/extrude-thin-open-curve.mitcad-ui (also run by
# app.ui-windows-extrude-thin-open-curve and
# app.ui-macos-extrude-thin-open-curve) on a design that mitcad-cli makes
# (tools/ui-scripts/extrude-thin-open-curve.json): a sketch with an open
# spline and a rectangle. Extrude starts with Type Extrude and the
# rectangle's profile; the click on the spline sets Type to Thin Extrude
# and takes the spline alone, and OK adds the wall along it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-thin-open-curve-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-thin-open-curve.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/extrude-thin-open-curve.mitcad-ui
"$cli" run "$here/ui-scripts/extrude-thin-open-curve.json" --save "$work/open-curve.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/open-curve.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'extrude thin open curve driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Extrude Profiles: open sketch curve taken, Type set to Thin Extrude' 'the open spline sets Thin Extrude'
ui_expect_log 'Added thin extrude (New Body), distance 10' 'the wall along the spline is added'
ui_expect_log 'TestDriver: done' 'an open spline clicked in Extrude makes a thin extrusion'
ui_finish 'UI open curve in Extrude sets Thin Extrude'
