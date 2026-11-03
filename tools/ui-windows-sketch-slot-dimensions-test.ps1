# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs
# A Center to Center Slot's typed length and width become its dimensions
# (mitcad#1225) in the build VM, as tools/ui-sketch-slot-dimensions-test.sh
# on Linux: the shared steps tools/ui-scripts/sketch-slot-dimensions.mitcad-ui
# on an empty sketch that mitcad-cli makes, then mitcad-cli's check of the
# saved design (tools/ui-scripts/sketch-slot-dimensions-check.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-slot-dimensions-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-slot-dimensions'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'slots.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-slot-dimensions.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'slot fixture creation failed' }
$workflow = Join-Path $work 'workflow.mitcad-ui'
$source = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/sketch-slot-dimensions.mitcad-ui')
Set-Content -Encoding ascii $workflow ($source.Replace('ENTER_SKETCH', "dblclick-logged '^Timeline Sketch1 at '"))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = $workflow
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'slot-dimensions' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'slot dimensions driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "slot dimensions driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "the slot's typed length and width are its dimensions, in one undo step"
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-slot-dimensions-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved slot's dimensions or degrees of freedom are wrong" }
Ui-StopApp
Write-Host 'PASS: slot typed dimensions'
