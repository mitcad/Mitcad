# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/order.rs core/sketch/src/steps.rs core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/dispatch.rs core/model/src/document.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# Sketch mode's list of the sketch's work steps in the timeline's place
# (mitcad#1750, after mitcad#1492's list of items) in the build VM, as
# tools/ui-sketch-item-list-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-item-list.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-item-list.json): a Project of a face's
# outline, a Rectangle and a dimension as three cells, icons only; the
# Project cell's hover list, a row's highlight and a row's click; the
# dimension's value editor from its cell; a refused Parallel's error badge
# on the Rectangle cell; the Rectangle's items selected from its cell and
# deleted with it; a sketch saved without steps shown one cell per item;
# and the timeline back after Finish Sketch.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-item-list-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-item-list'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'items.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-item-list.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-item-list.mitcad-ui'
Ui-StartApp 'sketch-item-list' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch step list driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch step list driver failed (exit code $exitCode)"
}
Ui-ExpectLog "Sketch item list in the timeline's place" "the step list in the timeline's place in sketch mode"
Ui-ExpectLog ' project x' 'the face projected as one Project step'
Ui-ExpectLog ' rectangle x' 'the rectangle as one step'
Ui-ExpectLog ' add_dimension x1' 'the dimension as one step'
Ui-ExpectLog ' hover list: Project: ' "the Project step's hover list"
Ui-ExpectLog ' clicked' 'an item selected from its row'
Ui-ExpectLog ' [error]' 'the conflict badge on its step'
Ui-ExpectLog 'Sketch steps: p1 point, p2 point, c3 line' 'a sketch without steps: one cell per item'
Ui-ExpectLog 'Timeline back in its place' 'the timeline back after the sketch'
Ui-ExpectLog 'TestDriver: done' 'the steps listed, hovered, selected, deleted and edited'
Ui-StopApp
Write-Host 'PASS: sketch step list'
