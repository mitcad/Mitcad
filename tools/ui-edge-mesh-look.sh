#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pictures of the view's lines zoomed in (mitcad#1633): a one-off visual
# check, not a recurring test (its name is not ui-*-test.sh, so no check
# picks it up). The steps tools/ui-scripts/edge-mesh-look.mitcad-ui (also
# run by tools/ui-windows-edge-mesh-look.ps1) on the design mitcad-cli
# makes (tools/ui-scripts/edge-mesh.json), with only test-driver steps
# older builds have too, so that the same script gives the pictures before
# and after the fix. The pictures (listed in the input script), the design
# and the app's log go to <dir>.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-edge-mesh-look.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-edge-mesh-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap '[ -n "${UI_LOG:-}" ] && cp -f "$UI_LOG" "$DIR/edge-mesh-look.log" 2> /dev/null; ui_cleanup' EXIT
export MITCAD_TEST_WORK=$DIR MITCAD_TEST_STEP_MS=20
export MITCAD_TEST_INPUT=$here/ui-scripts/edge-mesh-look.mitcad-ui
"$cli" run "$here/ui-scripts/edge-mesh.json" --save "$DIR/edge-mesh.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$DIR/edge-mesh.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'edge mesh look driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the pictures were taken'
for picture in "$DIR"/edge-mesh-*.png; do
  echo "ok   $picture"
done
