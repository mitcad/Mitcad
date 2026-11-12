# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchItemList.cpp app/sketch/SketchItemList.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchReveal.cpp
# check-all sources: app/sketch/SketchModel.cpp app/sketch/DrawTools.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/order.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-item-list.mitcad-ui tools/ui-scripts/sketch-item-list.json
# Sketch mode's list of the sketch's items in the timeline's place
# (mitcad#1492) in the build VM, as tools/ui-sketch-item-list-test.sh on
# Linux: the shared steps tools/ui-scripts/sketch-item-list.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/sketch-item-list.json): a
# rectangle with its dimensions listed in creation order, a line selected
# from its row, the view's selection marking a row, a constraint deleted
# from its row, the width edited from its row, and the timeline back after
# Finish Sketch.
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
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch item list driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch item list driver failed (exit code $exitCode)"
}
Ui-ExpectLog "Sketch item list in the timeline's place" "the list in the timeline's place in sketch mode"
Ui-ExpectLog 'Sketch items: c1 line, ' 'the rectangle listed in creation order'
Ui-ExpectLog 'Sketch item rows selected: c1' 'a line selected from its row'
Ui-ExpectLog 'Deleted k' 'a constraint deleted from its row'
Ui-ExpectLog 'Timeline back in its place' 'the timeline back after the sketch'
Ui-ExpectLog 'TestDriver: done' 'the rows in creation order, selected, deleted and edited'
Ui-StopApp
Write-Host 'PASS: sketch item list'
