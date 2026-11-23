#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPreferences.cpp app/browser/BrowserController.cpp
# check-all sources: app/view/ViewController.cpp app/view/PreferencesRestore.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/browser-control-places.mitcad-ui
# check-all sources: tools/ui-scripts/browser-control-places-docked.mitcad-ui
# check-all sources: tools/ui-scripts/browser-control-places.json
# The Browser rows' Activate radio button and Show eye places (mitcad#2223)
# on macOS, through the test driver (the same scripts as the Linux and
# Windows tests': tools/ui-scripts/browser-control-places.mitcad-ui in the
# Floating layout, Rows and Card, then browser-control-places-docked.mitcad-ui
# in the Docked one): with the defaults the radio buttons follow the names;
# on Right every radio button's and eye's centre has the same x at the
# visible right edge, the eye left of the radio button, and clicks activate
# and toggle; a long name is elided before them; Restore Defaults puts both
# back.
# Usage: tools/ui-macos-browser-control-places-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-browser-control-places}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/browser-control-places.json" --save "$UI_OUT/files/browser-control-places.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The two runs share the settings (MITCAD_SETTINGS_DIR of ui_init); the
# Floating run ends with Restore Defaults.
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-control-places.mitcad-ui \
  ui_run_app browser-control-places --ui=qt --no-recovery --no-mcp --chrome=floating \
  --open "$UI_OUT/files/browser-control-places.mitcad"
ui_expect_log 'TestDriver: done' 'the browser control places (Floating)'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion (Floating)'
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-control-places-docked.mitcad-ui \
  ui_run_app browser-control-places-docked --ui=qt --no-recovery --no-mcp --chrome=docked \
  --open "$UI_OUT/files/browser-control-places.mitcad"
ui_expect_log 'TestDriver: done' 'the browser control places (Docked)'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion (Docked)'
[ "$UI_FAILURES" = 0 ] || exit 1
