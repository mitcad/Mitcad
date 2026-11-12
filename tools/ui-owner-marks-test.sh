#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/framework/TestDriver.cpp core/model core/model-api tools/cli
# What is picked in the view marks its owners' rows in the browser and its
# feature in the timeline, without selecting them (mitcad#1704): the shared
# script tools/ui-scripts/owner-marks.mitcad-ui in the Docked layout, then
# in the Floating one (its Rows background).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-owner-marks.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/owner-marks.mitcad-ui
"$cli" run "$here/ui-scripts/owner-marks.json" --save "$work/owner-marks.mitcad" || exit 1
ui_start_display
for chrome in docked floating; do
  echo "--- $chrome"
  export MITCAD_TEST_CHROME=$chrome
  ui_start_app --ui=qt --no-mcp --chrome="$chrome" --open "$work/owner-marks.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "owner marks driver timed out ($chrome)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'TestDriver: done' "a face and a profile mark their owners, Delete acts on the face ($chrome)"
done
ui_finish 'UI owner marks of the view selection'
