#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-lathe-machine.mitcad-ui tools/ui-scripts/cam-lathe-machine.json
# A lathe made in Machines and Tools… (mitcad#2230, app/COMMANDS.md "The
# CNC group"), the shared input script
# tools/ui-scripts/cam-lathe-machine.mitcad-ui (also run by
# tools/ui-windows-cam-lathe-machine-test.ps1 and
# tools/ui-macos-cam-lathe-machine-test.sh) on a design without a machine
# (tools/ui-scripts/cam-lathe-machine.json): New Machine… > Lathe and its
# Edit Machine… (the turret typed), a turning tool and a parting tool added
# with the lathe's kinds, a lathe setup, Turn Facing and Generate.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-lathe-machine-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-lathe-machine.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-lathe-machine.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-lathe-machine.json" --save "$WORK/cam-lathe-machine.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-lathe-machine.mitcad"
for _ in $(seq 1 1000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM lathe machine driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "a lathe made in Machines and Tools, its tools, a lathe setup, Turn Facing, Generate"
ui_finish "UI CAM lathe machine: New Machine, the lathe's fields and tools, turning on it"
