# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/agent/MainWindowChat.cpp app/framework/Ribbon.cpp app/framework/WindowStyle.cpp app/framework/TestDriver.cpp
# The Docked toolbar's colours in the dark and the light theme (GitHub #127,
# mitcad#1245) in the build VM, as tools/ui-docked-toolbar-colors-test.sh on
# Linux: the shared steps tools/ui-scripts/docked-toolbar-colors.mitcad-ui.
# The Classic ribbon's body and the bar's Undo, Redo, separator and Chat
# buttons show the toolbar's surface (the "panels" colour of "Window
# style:") as the window draws them, in the dark theme, in the light one
# after a switch while Mitcad runs, and in the dark one again (the body was
# the Windows 11 style's neutral grey in the dark theme). Docked
# (--chrome=docked, Ui-StartApp's default).
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
Ui-ExpectLog 'TestDriver: widget background ribbonPageScroll' "the ribbon's body is logged"
Ui-ExpectLog 'TestDriver: done' "the ribbon's body and the bar's buttons on the toolbar's surface in both themes"
Get-Content $script:UiLog | Where-Object { $_ -like '*TestDriver: widget background*' } | ForEach-Object { Write-Host "     $_" }
Ui-StopApp
Write-Host 'PASS: the Docked toolbar on its surface in both themes'
