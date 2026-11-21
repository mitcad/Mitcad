# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandMessage.cpp app/MainWindowNoticeHover.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-conflict.mitcad-ui tools/ui-scripts/sketch-conflict.json
# check-all sources: tools/ui-scripts/sketch-conflict-replace.mitcad-ui tools/ui-scripts/sketch-conflict-replace.json
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/MainWindow.cpp
# check-all sources: app/files/MainWindowLinks.cpp
# A Sketch Fillet refused for conflicting constraints (mitcad#1566) on
# Windows, the shared input script tools/ui-scripts/sketch-conflict.mitcad-ui
# as tools/ui-sketch-conflict-test.sh runs it on Linux: Sketch Fillet R 2
# at the corner of the upper line and the left arc fails, the panel says
# in words, without ids, that the Equal can't hold with the fillet, and the
# Equal's symbol and lines are painted in the view, in the hover's colour
# while the pointer is over the message; Esc clears them. Then Replace
# (mitcad#2090) on tools/ui-scripts/sketch-conflict-replace.mitcad-ui, in
# the Docked layout (its status bar's button): a constraint or dimension
# refused for the one item that blocks it offers Replace, which swaps it
# out in one undo step; Shift at the click or Shift+Enter does so at once;
# no button for fixed geometry or several items; Esc takes it away; an
# Undo before the press replaces nothing; a constraint key never replaces
# at once.
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

# Replace (mitcad#2090), in the Docked layout.
$replace = Join-Path $work 'replace.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-conflict-replace.json') --save $replace
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the sketch conflict replace design was not made' }
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-conflict-replace.mitcad-ui'
Ui-StartApp 'sketch-conflict-replace' @('--ui=qt', '--no-mcp', '--chrome=docked', '--open', $replace)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'sketch conflict replace driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch conflict replace driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch conflict replace offered: k28' 'a refusal for one blocking item offers Replace'
Ui-ExpectLog 'Sketch conflict replaced: k28 by k' 'Replace swaps the Parallel for the Perpendicular'
Ui-ExpectLog 'Sketch conflict replaced (Shift): k28 by k' 'Shift at the click replaces at once'
Ui-ExpectLog 'Sketch conflict replace withdrawn' 'Esc takes the button away'
Ui-ExpectLog 'Sketch conflict replaced: k32 by k' 'Replace makes a driven dimension driving'
Ui-ExpectLog 'Sketch conflict replaced (Shift): k33 by k' 'Shift+Enter replaces at once'
Ui-ExpectLog 'Sketch conflict replace refused: the sketch changed since' 'a stale button replaces nothing'
Ui-ExpectLog 'TestDriver: done' 'Replace, Shift and their refusals'
# The sketch snapshots: the Parallel gone after Replace and back after
# Undo, the 20 mm dimension gone after its Replace.
function Has-Item([string]$File, [string]$Id) {
  return (Get-Content -Raw (Join-Path $work $File)).Contains('"' + $Id + '"')
}
if (Has-Item 'replace-constraint.json' 'k28') { Ui-Fail 'the Parallel k28 is still there after Replace' }
if (-not (Has-Item 'replace-constraint-undone.json' 'k28')) { Ui-Fail 'Undo did not bring the Parallel k28 back' }
if (Has-Item 'replace-dimension.json' 'k32') { Ui-Fail 'the 20 mm dimension k32 is still there after Replace' }
Write-Host 'ok   the sketch snapshots: k28 replaced and back after Undo, k32 replaced'
Ui-StopApp
Write-Host 'PASS: sketch conflicting constraints'
