# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# The Sketch Palette's Construction (X) box (mitcad#2040), with the driver
# script shared with Linux and macOS.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-construction-box'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'lines.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-construction-box.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'construction box fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-construction-box.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-construction-box' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'construction box driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "construction box driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the box converts the selection as X does and shows its state'
Ui-StopApp
Write-Host 'PASS: sketch construction box'
