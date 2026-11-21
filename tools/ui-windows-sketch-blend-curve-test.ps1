# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-blend-curve.mitcad-ui tools/ui-scripts/sketch-blend-curve.json
# Blend Curve (mitcad#2107) on Windows, the shared input script
# tools/ui-scripts/sketch-blend-curve.mitcad-ui as
# tools/ui-sketch-blend-curve-test.sh runs it on Linux: two lines blended
# with Tangent and two arcs with Curvature, each previewed and held by its
# two constraints, then a line's length dimensioned and the blend still
# joined.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-blend-curve'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'blend-curve.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-blend-curve.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch blend curve fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-blend-curve.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-blend-curve' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch blend curve driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch blend curve driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a tangent and a curvature blend made, held through a new dimension'
Ui-StopApp
Write-Host 'PASS: sketch Blend Curve tangent and curvature'
