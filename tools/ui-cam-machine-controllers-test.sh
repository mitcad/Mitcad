#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam core/model/src/cam core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-machine-controllers.mitcad-ui tools/ui-scripts/cam-machine-controllers.json
# The Controller list of Machines and Tools… > Edit Machine (mitcad#2119,
# app/COMMANDS.md "Machines and Tools…"), the shared input script
# tools/ui-scripts/cam-machine-controllers.mitcad-ui (also run by
# tools/ui-windows-cam-machine-controllers-test.ps1 and
# tools/ui-macos-cam-machine-controllers-test.sh) on a router and a lathe
# put by model commands (tools/ui-scripts/cam-machine-controllers.json):
# the router's list has the milling dialects and no lathe dialect, the
# lathe's only the lathe dialects.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-machine-controllers-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-machine-controllers.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-machine-controllers.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-machine-controllers.json" --save "$WORK/machines.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/machines.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM machine controllers driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "the router's and the lathe's controller lists"
grep -qE '^CAM machine controllers: router: .*turning' "$UI_LOG" &&
  ui_fail "the router's Controller list offers a lathe dialect"
ui_finish "UI CAM machine controllers: each machine's Controller list has its kind's dialects"
