#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli
# check-all sources: core/3mf core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/3mf-import.mitcad-ui tools/ui-scripts/3mf-import.json
# 3MF import (mitcad#2156) through the real UI, the shared input script
# tools/ui-scripts/3mf-import.mitcad-ui (also run by
# tools/ui-windows-3mf-import-test.ps1), on plate.3mf that mitcad-cli writes
# with MitCad's 3MF export (tools/ui-scripts/3mf-import.json: Plate placed
# once, Pin placed twice):
#   1. --open of plate.3mf: a new document of a mesh body and a component
#      with two occurrences, nothing asked;
#   2. File > Import of plate.3mf into a new design: Insert Mesh asks no
#      unit, the same bodies and component come in.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-3mf-import-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-3mf-import.XXXXXX")
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

(cd "$work" && "$cli" run "$here/ui-scripts/3mf-import.json") > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: plate.3mf"; exit 1; }
[ "$(head -c 2 "$work/plate.3mf")" = PK ] || { ui_fail "plate.3mf is no zip package"; exit 1; }
echo "ok   plate.3mf written by the 3MF export"
ui_start_display

MITCAD_TEST_INPUT=$here/ui-scripts/3mf-import.mitcad-ui \
  ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$work/plate.3mf"
wait_driver 'open and import'
ui_expect_log 'TestDriver: done' 'Open and File > Import of plate.3mf: a mesh body and a component of two occurrences'
ui_finish 'UI 3MF import'
