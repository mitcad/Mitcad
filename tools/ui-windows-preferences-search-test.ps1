# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreferencesSearch.cpp app/view/ViewController.cpp app/framework/SettingsWindow.cpp app/framework/CommandSearch.cpp app/framework/TestDriver.cpp tools/ui-scripts/preferences-search.mitcad-ui
# Preferences' search (mitcad#2074, GitHub mitcad/Mitcad#450) in the build
# VM, as tools/ui-preferences-search-test.sh on Linux: the shared steps
# tools/ui-scripts/preferences-search.mitcad-ui on a new design.
# "temperature" shows General with Temperature unit highlighted and the
# other pages greyed; Return focuses its box and Preferences stays open; a
# greyed page can be clicked and the text stays; "dark" finds Display's
# theme, "orbit" Navigation's bindings table; Esc clears; "3d mouse" opens
# Navigation's 3D Mouse tab; the command search's "temperature" lists
# Preferences: General and opens it with Temperature unit highlighted.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-preferences-search-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-preferences-search'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/preferences-search.mitcad-ui'
# Closes Preferences once its search field is empty (Cancel: nothing changed).
$env:MITCAD_TEST_PREFERENCES_DONE = 'Escape'
Ui-StartApp 'preferences-search' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'preferences search driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "preferences search driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preferences search: "temperature": ' 'the search field takes what is typed when Preferences opens'
Ui-ExpectLog 'Preferences search on General: Temperature unit' 'General shown with Temperature unit highlighted'
Ui-ExpectLog 'Preferences search: Return focuses temperatureUnit on General' 'Return focuses the first match'
Ui-ExpectLog 'Preferences search: "orbit": ' 'a table row is found'
Ui-ExpectLog 'Preferences search: "3d mouse": ' 'a sub-tab is found by its name'
Ui-ExpectLog 'Command search chose tools.preferences.general' 'the command search finds the page by its keyword'
Ui-ExpectLog 'TestDriver: done' 'counts, greyed pages, highlights, Return, Esc and the sub-tab as expected'
Ui-StopApp
Write-Host 'PASS: Preferences search'
