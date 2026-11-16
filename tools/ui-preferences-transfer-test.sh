#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/PreferencesTransfer.cpp app/view/PreferencesRestore.cpp app/view/ViewController.cpp
# check-all sources: app/view/ViewController.hpp app/framework/SettingsTransfer.cpp app/framework/SettingsTransfer.hpp
# check-all sources: app/framework/CommandRegistry.cpp app/view/NavigationBar.cpp app/framework/Ribbon.cpp
# check-all sources: app/MainWindow.cpp app/framework/SettingsWindow.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/preferences-transfer.mitcad-ui
# Preferences › Export Settings and Import Settings (mitcad#1889, GitHub
# mitcad/Mitcad#357) through the real UI, the shared input script
# tools/ui-scripts/preferences-transfer.mitcad-ui (also run by
# tools/ui-windows-preferences-transfer-test.ps1 and
# tools/ui-macos-preferences-transfer-test.sh) on a new design: a
# preference on Display, a keyboard shortcut and a navigation bar pin
# changed; Export Settings… with the default groups to a file; Restore
# Defaults › All Settings and the shortcut and pins back to their
# defaults; Import Settings… of the file brings all three back, shown,
# saved and in force (the pin on the bar, the key turning the view). The
# file is then checked as text: the header, LF line ends, no recent files.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preferences-transfer-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-preferences-transfer.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/preferences-transfer.mitcad-ui
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'preferences transfer driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preferences: small circles full' 'a preference changed on Display'
ui_expect_log 'Preferences: settings exported to ' 'Export Settings wrote the file'
ui_expect_log 'Preferences: restore defaults, all settings: ' 'every page back to its defaults'
ui_expect_log 'Preferences import settings asked: ' 'Import Settings asks first, saying what the file holds'
ui_expect_log 'Preferences: settings imported from ' 'Import Settings replaced the groups'
ui_expect_log 'Shortcuts reloaded from the settings: ' 'the keyboard shortcuts read again'
ui_expect_log 'Navigation bar pinned: solid.parameters, inspect.measure' 'the pin back on the navigation bar'
ui_expect_log 'View iso' 'the imported shortcut turns the view'
ui_expect_log 'TestDriver: done' 'the page shows the imported preference, the store holds all three'

file=$work/setup.mitcad-settings
if [ ! -f "$file" ]; then
  ui_fail 'the settings file was not written'
else
  grep -q '^\[MitCadSettings\]$' "$file" || ui_fail 'the settings file has no [MitCadSettings] section'
  grep -q '^version=1$' "$file" || ui_fail 'the settings file does not say settings version 1'
  grep -q 'view.iso=Shift+F8' "$file" || ui_fail 'the settings file does not hold the shortcut'
  grep -q 'navigationBarPins=' "$file" || ui_fail 'the settings file does not hold the pins'
  grep -q $'\r' "$file" && ui_fail 'the settings file has CR line ends'
  grep -q '^recentFiles=' "$file" && ui_fail 'the settings file holds recent files, which are left out by default'
  echo 'ok   the settings file: its header, the shortcut and the pins, LF line ends, no recent files'
fi
ui_finish 'UI preferences export and import settings'
