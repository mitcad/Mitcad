# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-break.mitcad-ui tools/ui-scripts/sketch-break.json
# Break and Split at Point (mitcad#2108) on Windows, the shared input script
# tools/ui-scripts/sketch-break.mitcad-ui as
# tools/ui-sketch-break-test.sh runs it on Linux: Split at Point splits an
# arc and keeps its radius dimension, Break splits a line crossed by a circle
# into three collinear pieces, and the pieces turn together when the line's
# free end is dragged.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-break'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'break.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-break.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch break fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
# Step 3's drag and click clear of the orientation cube (Ui-StepsPannedBefore).
$panned = Ui-StepsPannedBefore 'sketch-break.mitcad-ui' @("# 3. The free end dragged up: the pieces hold together.")
$source = Get-Content -Raw $panned
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-break' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch break driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch break driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'an arc split at a point, a line broken at its crossings, the pieces held together'
Ui-StopApp
Write-Host 'PASS: sketch Break and Split at Point'
