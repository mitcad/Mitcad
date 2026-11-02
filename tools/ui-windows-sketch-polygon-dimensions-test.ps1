# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-polygon-dimensions'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'polygons.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-polygon-dimensions.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'polygon fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-polygon-dimensions.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'polygon-dimensions' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'polygon dimensions driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "polygon dimensions driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'typed sizes, sides-only input and single-step Undo/Redo'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-polygon-dimensions-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'saved polygon dimensions have wrong targets or parameters' }
Ui-StopApp
Write-Host 'PASS: polygon typed dimensions'
