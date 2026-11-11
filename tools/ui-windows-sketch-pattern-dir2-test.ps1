# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/MainWindowSelection.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# The sketch Rectangular Pattern's second direction (mitcad#1640, GitHub
# mitcad/Mitcad#243) on Windows, the shared input script
# tools/ui-scripts/sketch-pattern-dir2.mitcad-ui as
# tools/ui-sketch-pattern-dir2-test.sh runs it on Linux: on a circle the
# pattern opens as a 3 x 2 grid (Quantity 2 of 2) with an arrow for each
# direction; dragging the second arrow changes Spacing 2; Quantity 2 of 1
# leaves that arrow faint.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-pattern-dir2'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'pattern.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-dir2.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch pattern fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-dir2.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-pattern-dir2' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch pattern driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch pattern driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a 3 x 2 grid, the second arrow dragged and faint'
Ui-StopApp
Write-Host "PASS: sketch Rectangular Pattern's second direction"
