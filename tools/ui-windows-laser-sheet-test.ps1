# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model/src/features/finger_joint.rs core/model/src/features/t_nut_slot.rs app/framework/TestDriver.cpp tools/cli tools/ui-scripts/laser-sheet.mitcad-ui tools/ui-scripts/laser-sheet.json
# Laser sheet joints (mitcad#1939) in the build VM, as tools/ui-laser-sheet-test.sh on Linux: the
# shared steps tools/ui-scripts/laser-sheet.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/laser-sheet.json). A finger joint of two plates at a corner selected in the browser, then a T-nut slot from a plate's end face with the screw's hole in its mate, with the volumes cli.sheet_laser_joints gives.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-laser-sheet-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-laser-sheet'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'laser-sheet.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/laser-sheet.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/laser-sheet.mitcad-ui'
Ui-StartApp 'laser-sheet' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'laser-sheet driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "laser-sheet driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added finger joint of 2 plates' 'the finger joint was added'
Ui-ExpectLog 'Added T-nut slot with the screw.s hole' 'the T-nut slot was added'
Ui-ExpectLog 'TestDriver: done' 'Laser sheet joints (mitcad#1939): every step of the script'
Ui-StopApp
Write-Host 'PASS: Laser sheet joints (mitcad#1939)'
