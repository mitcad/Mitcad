# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: core/sketch core/model/src/document/sketch_explode_text.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-explode-text.mitcad-ui tools/ui-scripts/sketch-explode-text.json
# Explode Text (mitcad#2163) in the build VM, as
# tools/ui-sketch-explode-text-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-explode-text.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-explode-text.json): the text
# right-clicked, Explode Text from its context menu, then Trim on a piece
# of the letter.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-explode-text-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-explode-text'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'text.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-explode-text.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-explode-text.mitcad-ui'
Ui-StartApp 'sketch-explode-text' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(170000)) { Ui-Fail 'sketch explode text driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch explode text driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Exploded text t1 in Sketch1: ' 'Explode Text from the context menu'
Ui-ExpectLog 'Trimmed c4 at ' 'Trim on a piece of the letter'
Ui-ExpectLog 'TestDriver: done' 'a text exploded and a piece of it trimmed'
Ui-StopApp
Write-Host 'PASS: sketch explode text'
