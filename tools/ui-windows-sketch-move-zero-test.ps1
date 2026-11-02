# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Move/Copy starts at 0 (mitcad#1163) on Windows, the shared input
# script tools/ui-scripts/sketch-move-zero.mitcad-ui as
# tools/ui-sketch-move-zero-test.sh runs it on Linux: a line selected and
# Move/Copy opened, X and Y Distance are 0 mm and the logged positions are
# the line's own; 10 mm typed moves it, OK keeps it; opened again it
# starts at 0 and the line stays; the saved sketch has the line moved once.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-move-zero'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'move.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-zero.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch Move/Copy fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-zero.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-move-zero' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch Move/Copy driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Move/Copy driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Move/Copy opened at 0, a typed distance, opened at 0 again'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-move-zero-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the saved line is not moved once by 10 mm' }
Ui-StopApp
Write-Host 'PASS: sketch Move/Copy starts at 0'
