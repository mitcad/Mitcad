#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp core/model/src/features/isoparametric_curve.rs geometry/src/iso_curve.cpp app/framework/ModelShapes.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/isoparametric-curve.mitcad-ui tools/ui-scripts/isoparametric-curve.json
# Isoparametric Curve (mitcad#2522) through the real UI: the shared input
# script tools/ui-scripts/isoparametric-curve.mitcad-ui (also run by
# tools/ui-windows-isoparametric-curve-test.ps1 and
# tools/ui-macos-isoparametric-curve-test.sh) on a cylinder that mitcad-cli
# makes (tools/ui-scripts/isoparametric-curve.json). The u curve through
# the clicked side face, a line along the axis; three v curves evenly
# spaced; the cylinder made taller and the first curve following it; the
# first curve reopened with its point.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-isoparametric-curve-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-isoparametric-curve.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/isoparametric-curve.mitcad-ui
design=$work/isoparametric-curve.mitcad
"$cli" run "$here/ui-scripts/isoparametric-curve.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'isoparametric-curve driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added isoparametric curve, U direction, through the picked point' 'the u curve was added through the click'
ui_expect_log 'Added isoparametric curve, V direction, 3 evenly spaced' 'three v curves were added'
ui_expect_log 'TestDriver: done' 'Isoparametric Curve (mitcad#2522): every step of the script'
ui_finish 'UI isoparametric-curve'
