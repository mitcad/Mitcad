#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BoostButton.cpp app/framework/Ribbon.cpp app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: tools/cli tools/ui-scripts/boost-button.mitcad-ui tools/ui-scripts/boost-button.json
# The Boost development button (mitcad#1990) in the Qt frontend on macOS,
# as tools/ui-boost-button-test.sh on Linux: the shared steps
# tools/ui-scripts/boost-button.mitcad-ui on a box that mitcad-cli makes
# (tools/ui-scripts/boost-button.json), in the Docked layout only (the
# tabs' row's right end; the Floating layout's macOS toolbar row has no
# Boost button): the button shown, a click logs the Ko-fi link (the tests'
# URL log, no browser), its × hides it and saves it, the Settings window's
# Display › Show the Boost development button brings it back.
# Usage: tools/ui-macos-boost-button-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-boost-button-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/boost.mitcad
"$cli" run "$here/ui-scripts/boost-button.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run boost-button.json' >&2; exit 1; }
MITCAD_TEST_LOG_URLS=1 MITCAD_TEST_INPUT=$here/ui-scripts/boost-button.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app boost-button-docked --ui=qt --no-recovery --no-mcp --chrome=docked --open "$design"
ui_expect_log '^Boost button shown$' 'the button shown'
ui_expect_log '^Boost link opened: https://ko-fi[.]com/mitcad$' 'a click opens the Ko-fi page'
ui_expect_log '^Open URL [(]test, not opened[)]: https://ko-fi[.]com/mitcad$' 'only logged in the test'
ui_expect_log '^Boost button hidden$' 'the × hides it'
ui_expect_log '^Preferences: Boost development button shown$' 'the Settings window shows it again'
ui_expect_log '^TestDriver: done' 'the Boost development button'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt Boost development button'
