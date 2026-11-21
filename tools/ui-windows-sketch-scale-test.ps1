# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/sketch/SketchController.cpp core/sketch core/solver core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-scale.mitcad-ui tools/ui-scripts/sketch-scale.json
# Sketch Scale (mitcad#2109) in the build VM, as
# tools/ui-sketch-scale-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-scale.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-scale.json): a circle and a line scaled by
# 1.5 about the center of the circle with their dimensions, undone, then
# scaled copies with Keep Values and Create Copy.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-scale-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-scale'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'scale.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-scale.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-scale.mitcad-ui'
Ui-StartApp 'sketch-scale' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Sketch Scale driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Sketch Scale driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Sketch Scale sizes: c3 radius 7.500' 'the circle scaled by 1.5'
Ui-ExpectLog 'Added Sketch Scale' 'OK kept Sketch Scale'
Ui-ExpectLog 'TestDriver: done' 'Sketch Scale scaled, undid, and scaled copies with the values kept'
Ui-StopApp
Write-Host 'PASS: Sketch Scale'
