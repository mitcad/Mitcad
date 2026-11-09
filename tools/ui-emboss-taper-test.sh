#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/EmbossCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/features/emboss.rs geometry/src/emboss.cpp geometry/src/emboss_taper.cpp
# check-all sources: tools/ui-scripts/emboss-taper.mitcad-ui tools/ui-scripts/emboss-taper.json
# Emboss's Taper Angle (mitcad#1547) through the real UI, on a design that
# mitcad-cli makes (tools/ui-scripts/emboss-taper.json: a rod of diameter
# 50 and a 10 x 10 square on a plane through its axis): the square wrapped
# onto the rod 45 deep with Taper Angle 10 typed in the panel, its ring
# shown, previewed, and OK logs the taper and grows the rod by the tapered
# emboss. The steps are tools/ui-scripts/emboss-taper.mitcad-ui, which
# tools/ui-windows-emboss-taper-test.ps1 runs on Windows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-emboss-taper-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-emboss-taper.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/emboss-taper.mitcad-ui
"$cli" run "$here/ui-scripts/emboss-taper.json" --save "$work/taper.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/taper.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'emboss taper driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added emboss (Raise) of 1 profile(s) on a cylindrical face, wrapped, depth 45, taper 10 deg' \
  'the tapered emboss added'
ui_expect_log 'TestDriver: done' 'Taper Angle set in the panel, previewed and kept by OK'
ui_finish 'UI emboss taper angle'
