# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveDragMotion.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/features/moves.rs tools/cli
# Move/Copy's Free Move handles (mitcad#1580, mitcad#1581, mitcad#1583,
# mitcad#1707) on
# Windows, the shared input script tools/ui-scripts/move-handles.mitcad-ui
# as tools/ui-move-handles-test.sh runs it on Linux: an edit's rings at the
# middle of what it moves, where the feature moves it, for a body, a
# component and a linked component (8650 mm along Y) and a turned body;
# OK without changes keeps the feature's matrix (the saved design checked
# with tools/ui-scripts/move-handles-check.json); with X Distance set the
# Y and Z arrows start at the moved middle, a drag of the Z arrow goes
# along Z from there; the arrows and rings in the axis colors; in the
# front view the X and Z arrows grabbed over the rings seen edge-on, in
# HandleActive while hovered and dragged (mitcad#1707).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-handles'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The linked part first, then the design that links it from this folder.
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-handles-part.json') --save (Join-Path $work 'part.mitcad')
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the linked part could not be made' }
$json = Join-Path $work 'design.json'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/move-handles.json')
Set-Content -Encoding ascii $json ($source.Replace('LINKED_DIR', ($work -replace '\\', '/')))
$design = Join-Path $work 'design.mitcad'
& $Cli run $json --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design could not be made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/move-handles.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'move-handles' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'Move/Copy handles driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Move/Copy handles driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the edits, the moved arrows and the axis colors'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-handles-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'OK without changes did not keep the matrix of the turned move' }
Ui-StopApp
Write-Host 'PASS: Move/Copy Free Move handles'
