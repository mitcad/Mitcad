#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowScripts.cpp app/framework/MacroRecorder.cpp app/framework/MacroRecorder.hpp
# check-all sources: app/framework/DocumentSession.cpp app/framework/TitleBar.cpp app/framework/TestDriver.cpp
# check-all sources: core/model-api/src/script.rs core/model-api/src/script_export.rs
# check-all sources: tools/ui-scripts/macro-record.mitcad-ui
# Macros and scripts (mitcad#2245) in the Qt frontend on macOS, as
# tools/ui-macro-record-test.sh on Linux: the shared steps
# tools/ui-scripts/macro-record.mitcad-ui in an empty design: Record
# Macro, a sketch, an extrusion and a fillet, Stop Recording and Save
# Macro…; the steps undone and Run Script… of the saved file, which builds
# the filleted block again as one undo step; File › Export Timeline as
# Script… writes the timeline. The macro and the exported script are then
# read here.
# Usage: tools/ui-macos-macro-record-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-macro-record-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
MITCAD_TEST_INPUT=$here/ui-scripts/macro-record.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app macro-record --ui=qt --no-recovery --no-mcp --no-native-dialogs
ui_expect_log '^Macro: recorded add_feature \(4 steps\)$' 'the sketch, the extrusion and the fillet recorded'
ui_expect_log '^Script: ran [0-9]+ steps from ' 'Run Script built the block again'
ui_expect_log '^Undo: Run fillet-block$' 'the script is one undo step'
macro=$MITCAD_TEST_WORK/fillet-block.json
if grep -q 'Macro recorded in MitCad' "$macro" && grep -q '"sketch.create"' "$macro" &&
   grep -q '"fillet"' "$macro"; then
  ui_pass 'the macro holds its comment, the sketch and the fillet'
else
  ui_fail "the macro $macro lacks its comment, the sketch or the fillet"
fi
timeline=$MITCAD_TEST_WORK/block-timeline.json
if grep -q '"add_feature"' "$timeline" && grep -q '"fillet"' "$timeline" && grep -q '"expect"' "$timeline"; then
  ui_pass 'the exported script adds the features and checks the bodies'
else
  ui_fail "the exported script $timeline lacks add_feature, the fillet or its expectations"
fi
ui_expect_log '^TestDriver: done' 'a macro recorded, saved and run; the timeline exported as a script'
ui_expect_no_log 'TestDriver: FAIL' 'no failed macro or script step'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt macros and scripts'
