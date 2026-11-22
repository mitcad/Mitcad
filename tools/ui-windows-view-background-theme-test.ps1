# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/view/PreferencesRestore.cpp app/framework/AppTheme.cpp app/framework/Theme.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp tools/ui-scripts/view-background-theme.mitcad-ui
# The 3D view's Follow System background follows MitCad's theme
# (mitcad#2145) in the build VM, as tools/ui-view-background-theme-test.sh
# on Linux: the shared steps tools/ui-scripts/view-background-theme.mitcad-ui.
# MitCad started with the setting appearance/theme=dark and no saved view
# background shows the dark gradient (#3b424d to #1c2027) under the dark
# window; Light and Dark chosen in Preferences repaint it at once; System
# follows the app mode (light, then its switch to dark); a chosen White
# Background stays; Restore Defaults > All Settings while the app mode is
# dark gives Follow System back, dark. Windows 11's own style, Floating.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-background-theme-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-background-theme'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
Remove-Item Env:MITCAD_TEST_COLOR_SCHEME -ErrorAction SilentlyContinue
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/view-background-theme.mitcad-ui'
$env:MITCAD_TEST_THEME_DESKTOP_DARK = 'dark'
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
# The Theme Dark; no view/background: the default, Follow System.
Set-Content -Encoding ascii $ini (@(Get-Content $ini) + @('[appearance]', 'theme=dark'))
Ui-StartApp 'view-background-theme' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'view background driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "view background driver failed (exit code $exitCode)"
}
Get-Content $script:UiLog | Where-Object {
  $_ -like '*Theme: *' -or $_ -like '*Background #*'
} | ForEach-Object { Write-Host "     $_" }
Ui-ExpectLog 'TestDriver: view Background #3b424d #1c2027' 'the dark gradient while the theme is dark'
Ui-ExpectLog 'TestDriver: view Background #f7f8fa #dfe3ea' 'the light gradient while the theme is light'
Ui-ExpectLog 'Preferences: restore defaults, all settings: ' 'Restore Defaults > All Settings'
Ui-ExpectLog 'TestDriver: done' 'Follow System: dark with the dark theme, light with the light one, after Restore Defaults too'
if (-not ((Get-Content $ini) -contains 'background=5')) { Ui-Fail 'Restore Defaults did not save Follow System as view/background' }
Write-Host 'ok   Follow System saved after Restore Defaults'
Ui-StopApp
Write-Host 'PASS: the view background follows the theme'
