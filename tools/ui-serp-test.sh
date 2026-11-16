#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli core/ffi/src/serp_test.rs
# check-all sources: core/serp core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/serp.mitcad-ui
# .serp files (mitcad#1870) through the real UI, the shared input script
# tools/ui-scripts/serp.mitcad-ui (also run by tools/ui-windows-serp-test.ps1
# and tools/ui-macos-serp-test.sh) on the import's test file written by
# mitcad-cli serp-test-file (B-reps OCCT wrote in its binary format, a
# mesh, a hidden object, a block placed twice): --open of it makes a new
# document of its five bodies, the block a component placed twice, nothing
# asked; File > Import of it into that design asks where it goes and makes
# a component named after the file; one undo step takes it back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-serp-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-serp.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100

# wait_driver what: waits for the driver's run of the application to end.
wait_driver() {
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
}

"$cli" serp-test-file "$work/sample.serp" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the test file"; exit 1; }
ui_start_display

MITCAD_TEST_INPUT=$here/ui-scripts/serp.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/sample.serp"
wait_driver 'open and import'
ui_expect_log 'TestDriver: done' 'File > Open of the .serp: its bodies and the block placed twice; File > Import asks where it goes'
ui_finish 'UI .serp open and import'
