# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-conic.mitcad-ui tools/ui-scripts/sketch-conic.json
# Conic Curve and Edit Rho (mitcad#2236) on Windows, the shared input
# script tools/ui-scripts/sketch-conic.mitcad-ui as
# tools/ui-sketch-conic-test.sh runs it on Linux: a conic drawn by three
# clicks with Rho 0.7 typed, then its Rho changed to 0.3 from its
# right-click menu.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-conic'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'conic.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-conic.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch conic fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-conic.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-conic' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch conic driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch conic driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a conic drawn with Rho 0.7 and its Rho changed to 0.3 from its menu'
Ui-StopApp
Write-Host 'PASS: sketch Conic Curve'
