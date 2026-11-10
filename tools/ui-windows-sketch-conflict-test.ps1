# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandMessage.cpp app/MainWindowNoticeHover.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-conflict.mitcad-ui tools/ui-scripts/sketch-conflict.json
# A Sketch Fillet refused for conflicting constraints (mitcad#1566) on
# Windows, the shared input script tools/ui-scripts/sketch-conflict.mitcad-ui
# as tools/ui-sketch-conflict-test.sh runs it on Linux: Sketch Fillet R 2
# at the corner of the upper line and the left arc fails, the panel says
# in words, without ids, that the Equal can't hold with the fillet, and the
# Equal's symbol and lines are painted in the view, in the hover's colour
# while the pointer is over the message; Esc clears them.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-conflict-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-conflict'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'conflict.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-conflict.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the sketch conflict design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-conflict.mitcad-ui'
Ui-StartApp 'sketch-conflict' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch conflict driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch conflict driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Panel Sketch Fillet error: the Equal between two lines can' 'the panel says what cannot hold, without ids'
Ui-ExpectLog 'Sketch conflict painted: k14 on c6, c9 (hovered)' 'the Equal is painted, in the hover colour over the message'
Ui-ExpectLog 'TestDriver: done' 'the refused fillet names and paints the Equal'
Ui-StopApp
Write-Host 'PASS: sketch conflicting constraints'
