# SPDX-License-Identifier: MIT
# check-all sources: core/model/src/build/follow.rs core/model/src/joints.rs app/MainWindowJoints.cpp
# check-all sources: app/commands/JointCommands.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/carriages-together.mitcad-ui
# Two bearings on one shaft joined by a Rigid joint or a Rigid Group move
# together (mitcad#2274, GitHub mitcad/Mitcad#575), in the build VM: the
# shared steps tools/ui-scripts/carriages-together.mitcad-ui (also run by
# tools/ui-carriages-together-test.sh and
# tools/ui-macos-carriages-together-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, github-575; never committed),
# unpacked into the test's folder: a Rigid joint between the bearings at
# their distance holds, a drag of one moves both, and a drag past one's
# travel limit stops both there; undone, the same with a Rigid Group.
# Skipped (77) without the design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-carriages-together-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-carriages-together'),
  [string]$QtBin = '')
$reportedRoot = $env:MITCAD_MITCAD_REPORTED
$zip = $null
if ($reportedRoot -and (Test-Path -LiteralPath (Join-Path $reportedRoot 'github-575'))) {
  $zip = Get-ChildItem -LiteralPath (Join-Path $reportedRoot 'github-575') -File -Filter '*.zip' |
    Sort-Object FullName | Select-Object -First 1
}
if (-not $zip) {
  Write-Host 'SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-575) is not here'
  exit 77
}
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$unpacked = Join-Path $work 'reported'
Expand-Archive -LiteralPath $zip.FullName -DestinationPath $unpacked -Force
$design = Get-ChildItem -LiteralPath $unpacked -Recurse -File -Filter '*.mitcad' | Sort-Object FullName | Select-Object -First 1
if (-not $design) { Ui-Fail 'no design in the reported zip' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/carriages-together.mitcad-ui'
Ui-StartApp 'carriages-together' @('--ui=qt', '--no-mcp', '--open', $design.FullName)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'carriages together driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "carriages together driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Joint Origin B: 1 face' "the Rigid joint picks both bearings' end faces"
Ui-ExpectLog ' (rigid)' 'the Rigid joint at their distance is added'
Ui-ExpectLog 'Added rigid group ' 'the Rigid Group of the two bearings is added'
Ui-ExpectLog 'Dragged ' 'the bearings are dragged along the shaft'
Ui-ExpectLog 'TestDriver: done' 'both move together and stop at the first travel limit'
Ui-StopApp
Write-Host 'PASS: bearings joined rigidly move together'
