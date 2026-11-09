# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/SketchController.cpp app/MainWindow.cpp
# check-all sources: core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-tangent-dimension.mitcad-ui tools/ui-scripts/sketch-tangent-dimension.json
# Sketch Dimension's right-click menu (mitcad#1540, GitHub #203) in the
# build VM, as tools/ui-sketch-tangent-dimension-test.sh on Linux: the
# shared steps tools/ui-scripts/sketch-tangent-dimension.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/sketch-tangent-dimension.json):
# Tangent from the right-click menu on a circle, the other circle by its
# centre, 18 typed: a tangent_point_distance; then both circles by their
# tangent sides and Driven: a driven tangent_circle_distance measuring 13.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-tangent-dimension-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-tangent-dimension'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'circles.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-tangent-dimension.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-tangent-dimension.mitcad-ui'
Ui-StartApp 'sketch-tangent-dimension' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'tangent dimension driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "tangent dimension driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Context menu: Center | Tangent | Driving (current) | Driven | Cancel' 'the right-click on a circle offers its menu'
Ui-ExpectLog 'Sketch Dimension preview: tangent_point_distance 20.00' 'the preview from the tangent side to the center'
Ui-ExpectLog 'tangent_point_distance' 'a tangent-to-center dimension added'
Ui-ExpectLog 'tangent_circle_distance = 13.00' 'a driven tangent-to-tangent dimension measuring 13'
Ui-ExpectLog 'TestDriver: done' 'tangent and center picks from the right-click menu'
Ui-StopApp
Write-Host 'PASS: sketch dimension tangent and center picks'
