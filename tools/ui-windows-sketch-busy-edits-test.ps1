# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/MainWindow.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverModel.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-busy-edits.mitcad-ui tools/ui-scripts/sketch-busy-edits.json
# Sketch edits asked for while a job computes the model (mitcad#2500,
# GitHub mitcad/Mitcad#706) in the build VM, as
# tools/ui-sketch-busy-edits-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-busy-edits.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-busy-edits.json): while a job of the
# test's own holds the model, Make Driven from a dimension's context menu
# and Delete on another dimension wait for it, then each is done in one
# undo step that one Undo takes back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-busy-edits-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-busy-edits'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sketch-busy-edits.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-busy-edits.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-busy-edits.mitcad-ui'
Ui-StartApp 'sketch-busy-edits' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'sketch busy edits driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch busy edits driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch Make Driven waits until the model is idle' 'Make Driven waits for the held job'
Ui-ExpectLog 'Undo: Make Driven in Sketch1' 'Make Driven after the job, one undo step'
Ui-ExpectLog 'Undo: Delete in Sketch1' 'Delete after the job, one undo step'
Ui-ExpectLog 'TestDriver: done' 'sketch edits while a job holds the model'
Ui-StopApp
Write-Host 'PASS: sketch edits while a job holds the model'
