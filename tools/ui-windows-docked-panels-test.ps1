# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/projects/MainWindowProjectsPanel.cpp app/browser/MainWindowConfigurationPanel.cpp app/framework/TestDriverDocks.cpp
# The Docked layout's panels hidden and shown again (mitcad#1455) in the
# build VM, as tools/ui-docked-panels-test.sh on Linux: the shared steps
# tools/ui-scripts/docked-panels.mitcad-ui. The Command dock hidden from
# the window's menu of its panels (a right-click on its title row) and with
# its close button comes back with View > Command each time; the Projects
# dock likewise with View > Projects; the window's menu offers no
# Configuration. Docked (--chrome=docked, Ui-StartApp's default).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-docked-panels-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-docked-panels'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/docked-panels.mitcad-ui'
Ui-StartApp 'docked-panels' @('--ui=qt', '--no-mcp', '--chrome=docked')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'the docked panels driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the docked panels driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: popup entries: Browser | Timeline | Projects | Command' "the window's menu of its panels offers no Configuration"
Ui-ExpectLog 'TestDriver: dock commandDock hidden, View/Command unchecked' 'the Command dock hidden'
Ui-ExpectLog 'TestDriver: dock projectsDock hidden, View/Projects unchecked' 'the Projects dock hidden'
Ui-ExpectLog 'TestDriver: done' 'the Command and Projects docks come back with the View menu'
Get-Content $script:UiLog | Where-Object { $_ -like '*TestDriver: popup entries*' -or $_ -like '*TestDriver: dock *' } | ForEach-Object { Write-Host "     $_" }
Ui-StopApp
Write-Host 'PASS: the Docked panels come back with the View menu'
