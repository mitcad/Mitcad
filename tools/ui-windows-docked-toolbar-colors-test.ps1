# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/agent/MainWindowChat.cpp app/framework/Ribbon.cpp app/framework/TitleBar.cpp app/framework/WindowStyle.cpp app/framework/TestDriver.cpp
# The Docked layout's top colours in the dark and the light theme (GitHub
# #127, mitcad#1245) in the build VM, as tools/ui-docked-toolbar-colors-test.sh
# on Linux: the shared steps tools/ui-scripts/docked-toolbar-colors.mitcad-ui.
# The top bar's title row and its Undo and Redo pair show the window's
# colour (the "window" colour of "Window style:"), its Chat button and the
# toolbar panel's card are of the theme (mitcad#2082: the Floating
# layout's top bar and panel), as the window draws them, in the dark theme,
# in the light one after a switch while Mitcad runs, and in the dark one
# again (the toolbar was the Windows 11 style's neutral grey in the dark
# theme). Docked (--chrome=docked, Ui-StartApp's default).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-docked-toolbar-colors-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-docked-toolbar-colors'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
Remove-Item Env:MITCAD_TEST_COLOR_SCHEME -ErrorAction SilentlyContinue
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/docked-toolbar-colors.mitcad-ui'
Ui-StartApp 'docked-toolbar-colors' @('--ui=qt', '--no-mcp', '--chrome=docked')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'the toolbar colours driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the toolbar colours driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: widget background titleRow' 'the title row is logged'
Ui-ExpectLog 'TestDriver: widget theme ribbonPanel' "the toolbar panel's card is logged"
Ui-ExpectLog 'TestDriver: done' 'the top bar and the toolbar panel in the theme colours in both themes'
Get-Content $script:UiLog | Where-Object { $_ -like '*TestDriver: widget background*' -or $_ -like '*TestDriver: widget theme*' } | ForEach-Object { Write-Host "     $_" }
Ui-StopApp
Write-Host 'PASS: the Docked top bar and toolbar panel in the theme colours in both themes'
