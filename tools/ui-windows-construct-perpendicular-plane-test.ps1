# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/MainWindowContextMenu.cpp app/framework/Command.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/features/construction
# check-all sources: core/model/src/features/coordinate_system.rs tools/cli
# Plane Perpendicular to Face at Point (mitcad#2111) in the build VM, as
# tools/ui-construct-perpendicular-plane-test.sh on Linux: the shared steps
# tools/ui-scripts/construct-perpendicular-plane.mitcad-ui on a design that
# mitcad-cli makes (tools/ui-scripts/construct-perpendicular-plane.json): a
# cylinder's side and a sketch point selected, the view's context menu
# offers the plane in Construct; made level through the side's nearest
# point, turned 90 degrees through the cylinder's axis by its Angle, and a
# sketch on it lies in it, upright.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-construct-perpendicular-plane-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-construct-perpendicular-plane'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}

$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'perpendicular.mitcad'
$output = & $Cli 'run' (Join-Path $PSScriptRoot 'ui-scripts/construct-perpendicular-plane.json') '--save' $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli run construct-perpendicular-plane.json: $($output -join ' / ')" }

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_LOG_PICKS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/construct-perpendicular-plane.mitcad-ui'
Ui-StartApp 'construct-perpendicular-plane' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'perpendicular plane driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "perpendicular plane driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Command Plane Perpendicular to Face at Point started' 'the context menu offers the plane for a face and a point'
Ui-ExpectLog 'Added plane Plane1: origin (10, 0, 10), normal (0, 0, -1)' "level through the side's nearest point"
Ui-ExpectLog 'Edited F3' 'the Angle changed'
Ui-ExpectLog 'Sketch frame in the design: origin (10.000000, 0.000000, 10.000000), normal (0.000000, 1.000000, 0.000000), x axis (-1.000000, 0.000000, 0.000000)' 'a sketch on the turned plane lies in it, upright'
Ui-ExpectLog 'TestDriver: done' 'the perpendicular plane made, turned and sketched on'
Ui-StopApp
Write-Host 'PASS: construct perpendicular plane'
