#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Pictures of the origin's Corner and Centered styles, with Size Screen and
# Size Model, and of a construction plane's resize handles (mitcad#1748): a
# one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). The steps
# tools/ui-scripts/origin-styles-look.mitcad-ui on a new design; the
# pictures (listed in the input script) and the app's log go to <dir>.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-origin-styles-look.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
DIR=${1:?usage: tools/ui-origin-styles-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
trap '[ -n "${UI_LOG:-}" ] && cp -f "$UI_LOG" "$DIR/origin-styles-look.log" 2> /dev/null; ui_cleanup' EXIT
export MITCAD_TEST_WORK=$DIR MITCAD_TEST_STEP_MS=20 MITCAD_LOG_PICKS=1
export MITCAD_TEST_INPUT=$here/ui-scripts/origin-styles-look.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'origin styles look driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the pictures were taken'
for picture in "$DIR"/origin-*.png; do
  echo "ok   $picture"
done
