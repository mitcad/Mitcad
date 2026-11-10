# SPDX-License-Identifier: MIT
# check-all sources: app/view/NavigationBar.cpp app/MainWindowNavigationBar.cpp app/drawings/DrawingWorkspace.cpp
# check-all sources: app/drawings/MainWindowDrawings.cpp app/MainWindowParameterEdits.cpp app/MainWindow.cpp tools/cli
# Edit Parameters one click away in every workspace (mitcad#1582, GitHub
# mitcad/Mitcad#222) in the build VM, as
# tools/ui-parameters-everywhere-test.sh on Linux: the shared steps
# tools/ui-scripts/parameters-everywhere.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/parameters-everywhere.json), Floating
# and then Docked: the navigation bar's Edit Parameters opens the parameter
# table in the SOLID, BUILD and MAKE workspaces and, as the sketch's entry,
# in a sketch, which stays open; in a drawing the drawing bar's Edit
# Parameters opens it over the drawing, which stays open.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-parameters-everywhere-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameters-everywhere'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'parameters.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameters-everywhere.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameters-everywhere.mitcad-ui'
foreach ($layout in @('floating', 'docked')) {
  Write-Host "--- $layout"
  Ui-StartApp "parameters-everywhere-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${layout}: the parameters driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the parameters driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Navigation bar: Edit Parameters' "${layout}: the navigation bar's Edit Parameters clicked"
  Ui-ExpectLog 'Parameters closed: back to Sketch1' "${layout}: the table opened from the bar in a sketch, the sketch kept"
  Ui-ExpectLog 'Drawing bar: Edit Parameters' "${layout}: the drawing bar's Edit Parameters clicked"
  Ui-ExpectLog 'TestDriver: done' "${layout}: Edit Parameters one click away in SOLID, BUILD, MAKE, a sketch and a drawing"
  Ui-StopApp
}
Write-Host 'PASS: Edit Parameters in every workspace'
