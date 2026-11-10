# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/framework/DocumentSession.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchPreferences.cpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-esc-leaves.mitcad-ui tools/ui-scripts/sketch-esc-leaves.json
# Esc leaves a sketch nothing was changed in (mitcad#1623) in the build VM,
# as tools/ui-sketch-esc-leaves-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-esc-leaves.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-esc-leaves.json): with the preference
# "Esc leaves an unchanged sketch" off (the default), Esc in a new sketch
# leaves nothing; with it on, Create Sketch and Esc
# leaves no sketch and no undo step; Edit Sketch and Esc leaves the sketch as
# it was; a line drawn keeps the sketch open on Esc, with a hint once;
# Ctrl+Z on the line, and Esc leaves it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-esc-leaves-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-esc-leaves'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'esc.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-esc-leaves.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-esc-leaves.mitcad-ui'
Ui-StartApp 'sketch-esc-leaves' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sketch Esc driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Esc driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: setting sketch/escLeavesUnchangedSketch = true' 'the preference off first (Esc left nothing), then turned on'
Ui-ExpectLog ' taken back, it was new and empty' 'Create Sketch, Esc: the new empty sketch taken back'
Ui-ExpectLog 'Esc: leaving Sketch1, nothing changed in it' 'Edit Sketch, Esc: the unchanged sketch left'
Ui-ExpectLog 'Esc: Sketch1 changed, it stays open; hint: ' 'a line drawn: Esc keeps the sketch open and says how to leave it'
$hints = @(Ui-LogLines | Where-Object { $_.StartsWith('Esc: Sketch1 changed, it stays open; hint: ') }).Count
if ($hints -ne 1) { Ui-Fail "the hint on leaving a changed sketch was given $hints times, not once" }
Write-Host 'ok   the hint given once'
Ui-ExpectLog 'TestDriver: done' 'Esc leaves an unchanged sketch and keeps a changed one'
Ui-StopApp
Write-Host 'PASS: sketch Esc leaves'
