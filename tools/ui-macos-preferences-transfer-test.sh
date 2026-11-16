#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/PreferencesTransfer.cpp app/framework/SettingsTransfer.cpp app/view/ViewController.cpp
# check-all sources: app/framework/SettingsWindow.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/preferences-transfer.mitcad-ui
# Preferences › Export Settings and Import Settings (mitcad#1889, GitHub
# mitcad/Mitcad#357) in the Settings window of the Qt frontend: the shared
# input script tools/ui-scripts/preferences-transfer.mitcad-ui of
# tools/ui-preferences-transfer-test.sh on a new design. A setting on
# Display, a keyboard shortcut and a navigation bar pin changed; Export
# Settings… with the default groups to a file; Restore Defaults › All
# Settings and the shortcut and pins back to their defaults; Import
# Settings… of the file brings all three back, shown in a new Settings
# window, saved and in force. The file is then checked as text: the
# header, LF line ends, no recent files.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" != 2 ]; then
  echo 'usage: ui-macos-preferences-transfer-test.sh <app> <output>' >&2
  exit 2
fi
ui_init "$1" "$2"
UI_TIMEOUT=300
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
file=$MITCAD_TEST_WORK/setup.mitcad-settings
[ ! -e "$file" ] || mv -f "$file" "$file.previous"
MITCAD_TEST_INPUT=$here/ui-scripts/preferences-transfer.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app preferences-transfer --ui=qt --no-native-dialogs --no-recovery --no-mcp
ui_expect_log 'Preferences: settings exported to ' 'Export Settings wrote the file'
ui_expect_log 'Preferences: restore defaults, all settings: ' 'every pane back to its defaults'
ui_expect_log 'Preferences import settings asked: ' 'Import Settings asks first, saying what the file holds'
ui_expect_log 'Preferences: settings imported from ' 'Import Settings replaced the groups'
ui_expect_log 'Navigation bar pinned: solid.parameters, inspect.measure' 'the pin back on the navigation bar'
ui_expect_log 'View iso' 'the imported shortcut turns the view'
ui_expect_log '^TestDriver: done' 'the pane shows the imported setting, the store holds all three'
ui_expect_no_log 'TestDriver: FAIL' 'no failed assertion'
if [ ! -f "$file" ]; then
  ui_fail 'the settings file was not written'
else
  grep -q '^\[MitCadSettings\]$' "$file" || ui_fail 'the settings file has no [MitCadSettings] section'
  grep -q '^platform=macos$' "$file" || ui_fail 'the settings file does not say it was written on macOS'
  grep -q 'view.iso=Shift+F8' "$file" || ui_fail 'the settings file does not hold the shortcut'
  ! grep -q $'\r' "$file" || ui_fail 'the settings file has CR line ends'
  ! grep -q '^recentFiles=' "$file" || ui_fail 'the settings file holds recent files, which are left out by default'
fi
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt preferences export and import settings'
