# SPDX-License-Identifier: MIT
# check-all sources: app/view/PreferencesRestore.cpp app/view/ViewController.cpp app/framework/SettingsWindow.cpp app/framework/TestDriver.cpp tools/ui-scripts/preferences-restore.mitcad-ui
# Preferences > Restore Defaults (mitcad#1887, GitHub mitcad/Mitcad#357) in
# the build VM, as tools/ui-preferences-restore-test.sh on Linux: the shared
# steps tools/ui-scripts/preferences-restore.mitcad-ui on a new design.
# Preferences changed on Display, General and Sketch; Restore Defaults >
# Current Group on Display, confirmed, puts only Display back to its
# defaults (shown at once and saved, the other pages keep their changes);
# > All Settings, confirmed, puts every page back, while a recent file and a
# library source stay.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-preferences-restore-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-preferences-restore'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/preferences-restore.mitcad-ui'
Ui-StartApp 'preferences-restore' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'preferences restore driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "preferences restore driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Orientation cube size 100 %' 'Cancel put the orientation cube size back (mitcad#2071)'
if ("$(Ui-LogText)".Contains('Preferences: orientation cube size ')) { Ui-Fail 'the cancelled cube size was saved' }
Ui-ExpectLog 'Preferences: small circles full' 'preferences changed on Display'
Ui-ExpectLog 'Preferences: click adds to the selection in sketches on' 'and on Sketch'
Ui-ExpectLog 'Preferences restore defaults asked: Restore the defaults of the Display settings?' 'Current Group asks first, naming the page'
Ui-ExpectLog 'Kept: recent files, library sources and fetched libraries, favorites, window sizes and places' 'the question says what is kept'
Ui-ExpectLog 'Preferences: restore defaults, current group: display' 'Display back to its defaults'
Ui-ExpectLog 'Preferences restore defaults asked: Restore the defaults of all settings?' 'All Settings asks first'
Ui-ExpectLog 'Preferences: restore defaults, all settings: general, navigation, display' 'every page back to its defaults'
Ui-ExpectLog 'TestDriver: done' 'the pages show the defaults, the store holds them, a recent file and a library source stay'
Ui-StopApp
Write-Host 'PASS: Preferences Restore Defaults'
