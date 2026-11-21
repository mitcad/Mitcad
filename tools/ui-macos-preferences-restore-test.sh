#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/PreferencesRestore.cpp app/view/ViewController.cpp app/framework/SettingsWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/preferences-restore.mitcad-ui
# Preferences › Restore Defaults (mitcad#1887, GitHub mitcad/Mitcad#357) in
# the Settings window of the Qt frontend: the shared input script
# tools/ui-scripts/preferences-restore.mitcad-ui of
# tools/ui-preferences-restore-test.sh on a new design. Settings changed on
# Display, General and Sketch; Restore Defaults › Current Group on Display,
# confirmed, puts only Display back to its defaults (shown at once in a new
# Settings window and saved, the other panes keep their changes);
# › All Settings, confirmed, puts every pane back, while a recent file and a
# library source stay.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-preferences-restore-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=300
MITCAD_TEST_INPUT=$here/ui-scripts/preferences-restore.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app preferences-restore --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log '^Preferences: orientation cube size 150 %$' 'the Settings window keeps the cube size, saved at once (mitcad#2071)'
ui_expect_log 'Preferences: restore defaults, current group: display' 'Display back to its defaults'
ui_expect_log 'Kept: recent files, library sources and fetched libraries' 'the question says what is kept'
ui_expect_log 'Preferences: restore defaults, all settings: general, navigation, display' 'every pane back to its defaults'
ui_expect_log '^TestDriver: done' 'the panes show the defaults, the store holds them, a recent file and a library source stay'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt preferences restore defaults'
