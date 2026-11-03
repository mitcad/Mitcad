# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# Sketch Fillet at a corner of a line and an arc (mitcad#1192) on Windows,
# the shared input script tools/ui-scripts/sketch-fillet-arc.mitcad-ui as
# tools/ui-sketch-fillet-arc-test.sh runs it on Linux: a half disc's corner
# point where its line and its arc end (two points held together, as a
# Trim leaves them) selected, Sketch Fillet
# takes it, previews and rounds the corner with the default radius 2; the
# saved sketch's one region is the half disc less the rounded-off corner.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-fillet-arc'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fillet.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-fillet-arc.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Sketch Fillet fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-fillet-arc.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-fillet-arc' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Sketch Fillet driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Sketch Fillet driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Sketch Fillet at the corner point of a line and an arc'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-fillet-arc-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the saved sketch has no fillet of r 2 at the corner of the half disc' }
Ui-StopApp
Write-Host 'PASS: Sketch Fillet of a line and an arc'
