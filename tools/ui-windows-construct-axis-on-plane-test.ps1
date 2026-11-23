# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/construction core/model/src/features/geom_ref.rs tools/cli
# Axis Perpendicular at Point on a plane (mitcad#2282) in the build VM, as
# tools/ui-construct-axis-on-plane-test.sh on Linux: the shared steps
# tools/ui-scripts/construct-axis-on-plane.mitcad-ui on a design that
# mitcad-cli makes (tools/ui-scripts/construct-axis-on-plane.json): its Face
# field takes a construction plane and the origin plane XY, and the axis
# goes through the point's projection onto the plane, along its normal.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-construct-axis-on-plane-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-construct-axis-on-plane'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}

$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'axis-on-plane.mitcad'
$output = & $Cli 'run' (Join-Path $PSScriptRoot 'ui-scripts/construct-axis-on-plane.json') '--save' $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli run construct-axis-on-plane.json: $($output -join ' / ')" }

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_LOG_PICKS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/construct-axis-on-plane.mitcad-ui'
Ui-StartApp 'construct-axis-on-plane' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'axis on plane driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "axis on plane driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added axis Axis1: origin (20, 0, 10), direction (0, 0, 1)' "the axis on a construction plane goes through the point's projection along its normal"
Ui-ExpectLog 'Added axis Axis2: origin (20, 0, 0), direction (0, 0, 1)' 'the axis on the origin plane XY'
Ui-ExpectLog 'TestDriver: done' 'both axes made on planes'
Ui-StopApp
Write-Host 'PASS: construct axis on plane'
