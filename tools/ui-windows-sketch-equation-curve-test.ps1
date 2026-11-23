# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/FormulaEditor.cpp app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-equation-curve.mitcad-ui tools/ui-scripts/sketch-equation-curve.json
# Equation Curve (mitcad#2237) on Windows, the shared input script
# tools/ui-scripts/sketch-equation-curve.mitcad-ui as
# tools/ui-sketch-equation-curve-test.sh runs it on Linux: an involute of a
# parameter made in the panel, its closed profile extruded, the parameter
# changed and the curve, the profile and the body following, then its
# formula edited.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-equation-curve'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'equation-curve.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-equation-curve.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch equation curve fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-equation-curve.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-equation-curve' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'sketch equation curve driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch equation curve driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'an involute of a parameter made, extruded, followed and edited'
Ui-StopApp
Write-Host 'PASS: sketch Equation Curve involute follows its parameter'
