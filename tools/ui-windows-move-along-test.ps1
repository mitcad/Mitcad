# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveDragMotion.cpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/features/moves.rs tools/cli
# Move/Copy's Point to Point along one axis (mitcad#2339) on Windows, the
# shared input script tools/ui-scripts/move-along.mitcad-ui as
# tools/ui-move-along-test.sh runs it on Linux: an edit of a Point to
# Point move of a cylinder onto a block's top corner shows Along (All, X,
# Y, Z) as icon buttons; Along Z chosen and OK, the saved design (checked
# with tools/ui-scripts/move-along-check.json) has along z and the
# cylinder moved up onto the top's level only.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-along'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'design.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-along.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design could not be made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/move-along.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'move-along' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Move/Copy Along driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Move/Copy Along driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the edit with Along Z'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-along-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Along Z did not keep the move to the Z distance only' }
Ui-StopApp
Write-Host 'PASS: Move/Copy Point to Point Along'
