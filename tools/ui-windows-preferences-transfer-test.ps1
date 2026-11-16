# SPDX-License-Identifier: MIT
# check-all sources: app/view/PreferencesTransfer.cpp app/framework/SettingsTransfer.cpp app/view/ViewController.cpp app/framework/TestDriver.cpp tools/ui-scripts/preferences-transfer.mitcad-ui
# Preferences > Export Settings and Import Settings (mitcad#1889, GitHub
# mitcad/Mitcad#357) in the build VM, as tools/ui-preferences-transfer-test.sh
# on Linux: the shared steps tools/ui-scripts/preferences-transfer.mitcad-ui
# on a new design. A preference on Display, a keyboard shortcut and a
# navigation bar pin changed; Export Settings with the default groups to a
# file; Restore Defaults > All Settings and the shortcut and pins back to
# their defaults; Import Settings of the file brings all three back, shown,
# saved and in force. The file is then checked as text: the header, LF line
# ends (the same file on every platform), no recent files.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-preferences-transfer-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-preferences-transfer'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$settingsFile = Join-Path $work 'setup.mitcad-settings'
if (Test-Path $settingsFile) { Remove-Item -Force $settingsFile }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/preferences-transfer.mitcad-ui'
Ui-StartApp 'preferences-transfer' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'preferences transfer driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "preferences transfer driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preferences: small circles full' 'a preference changed on Display'
Ui-ExpectLog 'Preferences: settings exported to ' 'Export Settings wrote the file'
Ui-ExpectLog 'Preferences: restore defaults, all settings: ' 'every page back to its defaults'
Ui-ExpectLog 'Preferences import settings asked: ' 'Import Settings asks first, saying what the file holds'
Ui-ExpectLog 'Preferences: settings imported from ' 'Import Settings replaced the groups'
Ui-ExpectLog 'Shortcuts reloaded from the settings: ' 'the keyboard shortcuts read again'
Ui-ExpectLog 'Navigation bar pinned: solid.parameters, inspect.measure' 'the pin back on the navigation bar'
Ui-ExpectLog 'View iso' 'the imported shortcut turns the view'
Ui-ExpectLog 'TestDriver: done' 'the page shows the imported preference, the store holds all three'
if (-not (Test-Path $settingsFile)) { Ui-Fail 'the settings file was not written' }
$bytes = [System.IO.File]::ReadAllBytes($settingsFile)
$text = [System.Text.Encoding]::UTF8.GetString($bytes)
if ($bytes -contains 13) { Ui-Fail 'the settings file has CR line ends' }
if ($text -notmatch '(?m)^\[MitCadSettings\]$') { Ui-Fail 'the settings file has no [MitCadSettings] section' }
if ($text -notmatch '(?m)^platform=windows$') { Ui-Fail 'the settings file does not say it was written on Windows' }
if ($text -notmatch 'view\.iso=Shift\+F8') { Ui-Fail 'the settings file does not hold the shortcut' }
if ($text -match '(?m)^recentFiles=') { Ui-Fail 'the settings file holds recent files, which are left out by default' }
Ui-StopApp
Write-Host 'PASS: Preferences Export Settings and Import Settings'
