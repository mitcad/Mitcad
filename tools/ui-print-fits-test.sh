#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/print_fits.rs core/model-tests/print/src/print_fits_tests.rs
# The shared printed-fit workflow, on a private Xvfb display.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-print-fits.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
"$CLI" run "$here/ui-scripts/printed-fits.json" --save "$WORK/shaft.mitcad" > "$WORK/setup.log" 2>&1 || {
  cat "$WORK/setup.log" >&2
  exit 1
}
ui_start_display
MITCAD_TEST_INPUT=$here/ui-scripts/printed-fits.mitcad-ui MITCAD_TEST_STEP_MS=70 \
  ui_start_app --ui=qt --no-mcp --open "$WORK/shaft.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2>/dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2>/dev/null && { ui_fail 'printed-fit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'printed-fit fields, parameters and undo passed'
echo 'PASS: printed fits'
