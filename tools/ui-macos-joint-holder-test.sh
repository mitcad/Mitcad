#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJointHolder.cpp app/browser/JointHolderQuestion.cpp tools/cli tools/ui-scripts/joint-holder.mitcad-ui
# Joint's picks under another component than the active one (mitcad#1856): the shared input script
# tools/ui-scripts/joint-holder.mitcad-ui of tools/ui-joint-holder-test.sh in
# the Qt frontend, on the design that mitcad-cli makes
# (tools/ui-scripts/joint-holder.json): the question offers the root,
# which holds both picks, Make it in mounts the roller whole, Don't ask
# again sets the preference and Preferences > Assembly sets it back to Ask;
# a selection made before Joint asks too (mitcad#1864); with the root
# active, picks inside one sub-assembly ghost only side A's own bodies and
# ask for that sub-assembly (mitcad#2027).
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-joint-holder-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=540
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/joint-holder.json" --save "$MITCAD_TEST_WORK/joint-holder.mitcad" || exit 1
MITCAD_TEST_INPUT=$here/ui-scripts/joint-holder.mitcad-ui MITCAD_TEST_STEP_MS=100   ui_run_app joint-holder --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/joint-holder.mitcad"
ui_expect_log '^TestDriver: done' 'the joint goes into the component that holds its picks'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt joint holder'
