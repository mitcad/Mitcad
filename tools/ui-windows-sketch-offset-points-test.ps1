# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/DrawTools.cpp app/sketch/SketchController.cpp app/sketch/SketchModel.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-offset-points.mitcad-ui tools/ui-scripts/sketch-offset-points.json
# Lines started and ended at the ends of a derived offset of a spline are
# held there as the source moves (mitcad#2278) in the build VM, as
# tools/ui-sketch-offset-points-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-offset-points.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-offset-points.json): the Line
# tool from the offset's start to the spline's start and from the spline's
# end to the offset's end closes a profile with nothing refused, and a drag
# of the spline's end keeps it closed.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-offset-points-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-offset-points'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'offset-points.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-offset-points.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-offset-points.mitcad-ui'
Ui-StartApp 'sketch-offset-points' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'offset points driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "offset points driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Dragged p7 to ' 'the spline end was dragged'
Ui-ExpectLog 'TestDriver: done' 'lines at a derived offset end are held there as the source moves'
Ui-StopApp
Write-Host 'PASS: sketch lines held at a derived offset end'
