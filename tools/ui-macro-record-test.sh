#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowScripts.cpp app/framework/MacroRecorder.cpp app/framework/MacroRecorder.hpp
# check-all sources: app/framework/DocumentSession.cpp app/framework/TitleBar.cpp app/framework/TestDriver.cpp
# check-all sources: core/model-api/src/script.rs core/model-api/src/script_export.rs
# check-all sources: tools/ui-scripts/macro-record.mitcad-ui
# Macros and scripts (mitcad#2245) through the real UI, the shared input
# script tools/ui-scripts/macro-record.mitcad-ui (also run by
# tools/ui-windows-macro-record-test.ps1 and
# tools/ui-macos-macro-record-test.sh) in an empty design: Record Macro, a
# sketch, an extrusion and a fillet, Stop Recording and Save Macro…; the
# steps undone and Run Script… of the saved file, which builds the
# filleted block again as one undo step; File › Export Timeline as
# Script… writes the timeline. The macro and the exported script are then
# read here.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-macro-record-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-macro-record.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/macro-record.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'macro recording driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Macro: recorded add_feature (4 steps)' 'the sketch, the extrusion and the fillet recorded'
ui_expect_log 'Script: ran ' 'Run Script built the block again'
ui_expect_log 'Undo: Run fillet-block' 'the script is one undo step'
# The macro: its comment first, then the commands as the application sent
# them.
macro=$work/fillet-block.json
if grep -q 'Macro recorded in MitCad' "$macro" && grep -q '"sketch.create"' "$macro" &&
   grep -q '"fillet"' "$macro"; then
  echo 'ok   the macro holds its comment, the sketch and the fillet'
else
  ui_fail "the macro $macro lacks its comment, the sketch or the fillet"
fi
# The exported timeline: each feature by add_feature, the fillet among them.
timeline=$work/block-timeline.json
if grep -q '"add_feature"' "$timeline" && grep -q '"fillet"' "$timeline" && grep -q '"expect"' "$timeline"; then
  echo 'ok   the exported script adds the features and checks the bodies'
else
  ui_fail "the exported script $timeline lacks add_feature, the fillet or its expectations"
fi
ui_expect_log 'TestDriver: done' 'a macro recorded, saved and run; the timeline exported as a script'
ui_finish 'UI macros and scripts'
