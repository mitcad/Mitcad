# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandPanel.cpp app/MainWindowSelection.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# The sketch's x and y axes as the sketch Rectangular Pattern's Direction
# (mitcad#1655, GitHub mitcad/Mitcad#248) on Windows, the shared input
# script tools/ui-scripts/sketch-pattern-axis.mitcad-ui as
# tools/ui-sketch-pattern-axis-test.sh runs it on Linux: in a sketch of a
# circle and no line, the y axis picked as Direction runs the copies along
# y and its field names it; the x axis picked runs them along x.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-pattern-axis'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'pattern.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-dir2.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch pattern fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-axis.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-pattern-axis' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch pattern axis driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch pattern axis driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the copies along the picked y axis, then the x axis'
Ui-StopApp
Write-Host "PASS: sketch Rectangular Pattern's Direction along the sketch's axes"
