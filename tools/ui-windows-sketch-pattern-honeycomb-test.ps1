# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandPanel.cpp app/MainWindowSelection.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# The sketch Rectangular Pattern's Direction 2 (mitcad#1969, GitHub
# mitcad/Mitcad#406) on Windows, the shared input script
# tools/ui-scripts/sketch-pattern-honeycomb.mitcad-ui as
# tools/ui-sketch-pattern-honeycomb-test.sh runs it on Linux: a line at 60
# degrees picked as Direction 2 runs the second row of copies along it;
# Edit Pattern keeps it, and At Right Angles turns the row back.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-pattern-honeycomb'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'pattern.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-honeycomb.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch pattern fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-pattern-honeycomb.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-pattern-honeycomb' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch pattern honeycomb driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch pattern honeycomb driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the second row along the picked 60 degree line, kept by Edit Pattern'
Ui-StopApp
Write-Host "PASS: sketch Rectangular Pattern's Direction 2 at 60 degrees"
