# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchPreferences.cpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriverOrbitCenter.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-scale-view.mitcad-ui tools/ui-scripts/sketch-scale-view.json
# The view follows a sketch its first dimension scales (mitcad#1948,
# GitHub #400) in the build VM, as tools/ui-sketch-scale-view-test.sh on
# Linux: the shared steps tools/ui-scripts/sketch-scale-view.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/sketch-scale-view.json):
# a diameter of 50 scales the sketch by 1.25, then (undone) by 0.8, and the
# dimensioned circle's centre stays at its place in the window within a
# pixel, at the same zoom.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-scale-view-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-scale-view'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'circles.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-scale-view.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-scale-view.mitcad-ui'
Ui-StartApp 'sketch-scale-view' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch scale view driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch scale view driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Scaled the sketch by 1.25 to its first dimension' 'the first diameter scaled the sketch up by 1.25'
Ui-ExpectLog 'Scaled the sketch by 0.8 to its first dimension' 'the first diameter scaled the sketch down by 0.8'
Ui-ExpectLog 'Sketch view moved with the scaled sketch by 150.000 mm' 'the view followed the circle scaled up'
Ui-ExpectLog 'Sketch view moved with the scaled sketch by 144.000 mm' 'the view followed the circle scaled down'
Ui-ExpectLog 'TestDriver: done' 'the dimensioned circle kept its place in the window'
Ui-StopApp
Write-Host 'PASS: sketch view follows the first-dimension scale'
