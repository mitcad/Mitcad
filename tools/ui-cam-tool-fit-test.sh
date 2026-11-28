#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam/CamToolFit.cpp app/cam/CamToolFit.hpp app/cam/MainWindowCam.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/cam/src/ops core/model/src/cam core/model/src/document/cam.rs tools/cli
# check-all sources: tools/ui-scripts/cam-tool-fit.mitcad-ui tools/ui-scripts/cam-tool-fit.json
# The milling panels' Tool (mitcad#2590, GitHub mitcad/Mitcad#748;
# app/COMMANDS.md "The CNC group"), the input script
# tools/ui-scripts/cam-tool-fit.mitcad-ui on a router whose tool 1 is a
# ball end mill and tool 2 a flat end mill (tools/ui-scripts/cam-tool-fit.json):
# a saved pocket's ball end mill flagged and replaced; Pocket, Contour,
# Facing starting with the flat end mill and generating, Adaptive Clearing
# starting with it; Contour on the line taking the engraving cutter; a setup
# whose machine has no end mill refused at Tool.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-tool-fit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-tool-fit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-tool-fit.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-tool-fit.json" --save "$WORK/cam-tool-fit.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-tool-fit.mitcad"
for _ in $(seq 1 1000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "CAM tool fit driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' "a saved ball end mill replaced, the flat end mill by default, the engraving cutter on the line, a setup without an end mill"
ui_finish "UI CAM tool fit: the milling panels offer and start with the cutters their operations take"
