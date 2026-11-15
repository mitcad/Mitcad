# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/ManipulatorOverlay.cpp app/framework/TestDriver.cpp tools/cli core/model/src/features/press_pull.rs core/model-api/src/press_pull.rs geometry/src/press_pull.cpp
# Press Pull on a face continued (mitcad#1850) in the build VM, as
# tools/ui-press-pull-continued-test.sh on Linux: the shared steps
# tools/ui-scripts/press-pull-continued.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/press-pull-continued.json): on a bent
# tube's end Continue offers Revolve1's own edit; Along the Walls with its
# ring dragged past the walls' inner radius, then 25 mm; Straight with a
# Scale taper of 1.1 on a T-slot bar's end.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-press-pull-continued-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-press-pull-continued'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'press-pull-continued.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/press-pull-continued.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/press-pull-continued.mitcad-ui'
Ui-StartApp 'press-pull-continued' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'press pull continued driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "press pull continued driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Press Pull opened Revolve1 for its angle' 'the tube''s end opened Revolve1 at its Angle'
Ui-ExpectLog 'Press Pull made press pull along the walls, distance 25' 'Along the Walls on the bent tube''s end'
Ui-ExpectLog 'Press Pull made press pull straight, distance 10' 'Straight with a Scale taper on the T-slot bar'
Ui-ExpectLog 'TestDriver: done' 'the bent tube and the T-slot bar continued'
Ui-StopApp
Write-Host 'PASS: Press Pull continues a face Straight or Along the walls'
