# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchPalette.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-show-points.mitcad-ui tools/ui-scripts/sketch-show-points.json
# The Sketch Palette's Show points (mitcad#1705) in the build VM, as
# tools/ui-sketch-show-points-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-show-points.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-show-points.json): on (the default), every
# point's marker is drawn; unchecked, none is, while the end point under the
# pointer still shows as the hover, a drag moves it and a click selects a
# point and draws its ball; the next edit of the sketch opens with it off
# again, and checked again every marker is drawn.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-show-points-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-show-points'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'points.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-show-points.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-show-points.mitcad-ui'
Ui-StartApp 'sketch-show-points' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sketch show points driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch show points driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch Sketch1 opens with Show points on' 'on (the default): Sketch1 opens with its points drawn'
Ui-ExpectLog 'Show points off' 'the palette hides the points'
Ui-ExpectLog '0 markers over them' 'a hidden end point shows under the pointer'
Ui-ExpectLog 'Sketch Sketch1 opens with Show points off' 'off is remembered for the next edit'
Ui-ExpectLog 'Show points on' 'the palette draws the points again'
Ui-ExpectLog 'TestDriver: done' 'Show points in the Sketch Palette'
Ui-StopApp
Write-Host 'PASS: sketch show points'
