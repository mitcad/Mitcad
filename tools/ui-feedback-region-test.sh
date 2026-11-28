#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/report app/framework/TestDriver.cpp app/platform/MacChrome.hpp tools/cli
# check-all sources: tools/ui-scripts/feedback-region.mitcad-ui tools/ui-scripts/feedback-region.json
# Send Feedback on a Region (mitcad#2616), the shared input script
# tools/ui-scripts/feedback-region.mitcad-ui (also run by
# tools/ui-windows-feedback-region-test.ps1 and
# tools/ui-macos-feedback-region-test.sh): F9 pressed once, then a drag
# from the 3D view to a browser row on the overlay, which the view does not
# see; Send Feedback's form opens with Bug, the region's screenshot at its
# size and private, the description with the keyboard; Esc and a click
# without a drag cancel with no form; F9 held and released after the drag
# takes the region. Here the screen's device pixel ratio is 1: the
# picture's size is the region's.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-feedback-region-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-feedback-region.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/feedback-region.mitcad-ui
"$CLI" run "$here/ui-scripts/feedback-region.json" --save "$WORK/region.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/region.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'feedback region driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Feedback region: picking' 'the overlay shows on F9'
ui_expect_log 'Feedback form for a region: kind bug, screenshot on, the description has the keyboard' \
  'the form for a region'
ui_expect_log ', screenshot private' 'the region screenshot is private'
ui_expect_log 'Feedback region: canceled (Esc)' 'Esc cancels'
ui_expect_log 'Feedback region: canceled (a click without a drag)' 'a click without a drag cancels'
# Device pixel ratio 1 on Xvfb: each picture is as large as its region, from
# the screen.
regions=$(grep -o 'Feedback region: [0-9]*x[0-9]* at ' "$UI_LOG" | sed 's/^Feedback region: //; s/ at $//')
images=$(grep -o 'Feedback region image: [0-9]*x[0-9]* from the screen$' "$UI_LOG" |
  sed 's/^Feedback region image: //; s/ from the screen$//')
[ "$(wc -l <<< "$regions")" = 2 ] || ui_fail "two regions expected: $regions"
[ "$regions" = "$images" ] || ui_fail "the pictures ($images) are not the regions' size ($regions)"
echo "ok   the pictures from the screen at the regions' sizes: $(tr '\n' ' ' <<< "$images")"
ui_expect_log 'TestDriver: done' 'Send Feedback on a Region'
ui_finish 'UI feedback region'
