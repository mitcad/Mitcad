# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/MainWindowSelection.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Move/Copy (mitcad#1563, GitHub mitcad/Mitcad#212) on Windows, the
# shared input script tools/ui-scripts/sketch-move-p2p.mitcad-ui as
# tools/ui-sketch-move-p2p-test.sh runs it on Linux: Point to Point moves
# a rectangle from its corner to a circle's center, both picked with the
# sketch's snaps; Translate's handle dragged onto a point sets X and Y
# Distance; Rotate without a Pivot Point turns a horizontal line 90
# degrees about its middle.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-move-p2p'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'move.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-p2p.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch Move/Copy fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-p2p.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-move-p2p' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch Move/Copy driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Move/Copy driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Point to Point, the handle and Rotate about the middle'
Ui-StopApp
Write-Host 'PASS: sketch Move/Copy Point to Point, handle and pivot'
