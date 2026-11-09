# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch_loop.rs core/model-api/src/lib.rs tools/cli
# A double-click on a sketch curve selects its closed loop, else its open
# chain (mitcad#1541) on Windows, the shared input script
# tools/ui-scripts/sketch-double-click-chain.mitcad-ui as
# tools/ui-sketch-double-click-chain-test.sh runs it on Linux: a rectangle
# side selects the rectangle's four lines and not the line leaving its
# corner, that line only itself, an open polyline's line the whole
# polyline, and Ctrl+double-click adds the rectangle to the selection.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-double-click-chain'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'chain.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-double-click-chain.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'sketch double-click chain fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-double-click-chain.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-double-click-chain' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch double-click chain driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch double-click chain driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a double-click selects a closed loop past a branch, else an open chain'
Ui-StopApp
Write-Host 'PASS: sketch double-click chain'
