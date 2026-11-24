# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn.mitcad-ui tools/ui-scripts/cam-mill-turn.json
# CAM mill-turn in the CNC group (mitcad#2067) on Windows, the shared input
# script tools/ui-scripts/cam-mill-turn.mitcad-ui as
# tools/ui-cam-mill-turn-test.sh runs it on Linux: a lathe setup with live
# tools; Turn Facing and a Drill on the face from their panels, a pocket on
# the face and a cross hole on the diameter; Generate; Simulate (the live
# tools' cuts in words); Post Process... to the Fanuc turning dialect (the C
# axis on and off, feeds per minute, the clamp, polar interpolation, G87).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam-mill-turn'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam-mill-turn.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam-mill-turn.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM mill-turn fixture creation failed' }

$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam-mill-turn.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam-mill-turn' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'CAM mill-turn driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM mill-turn driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a lathe setup, turning and live tool operations, generate, simulate and post'

# The Fanuc program with the C axis.
$file = Join-Path $work 'setup1.nc'
if (-not (Test-Path $file)) { Ui-Fail 'no program written' }
$lines = @(Get-Content $file)
if ($lines[0] -ne '%') { Ui-Fail "the program does not start with the tape mark: $($lines[0])" }
foreach ($want in @('G21 G18 G40 G99', 'G96 S200 M3', 'M5', 'M154', 'G28 H0.', 'G98', 'M133 P2000', 'M14', 'M15',
    'G17', 'G12.1', 'G13.1', 'M135', 'M155', 'G99')) {
  if ($lines -notcontains $want) { Ui-Fail "no '$want' in the program" }
}
foreach ($pattern in @('^G0 X[0-9.]+ C-?[0-9.]+$', '^G87 Z-5[.] C0[.] X30[.] R-?[0-9.]+ F[0-9.]+$')) {
  if (@($lines | Where-Object { $_ -match $pattern }).Count -eq 0) { Ui-Fail "no line like $pattern in the program" }
}
foreach ($name in @('TURN FACING 1', 'DRILL 1', 'POCKET 1', 'DRILL 2')) {
  if ($lines -notcontains "($name)") { Ui-Fail "no $name in the program" }
}
if ($lines[-2] -ne 'M30' -or $lines[-1] -ne '%') { Ui-Fail 'the program does not end with M30 and the tape mark' }
Write-Host 'ok   the Fanuc program with the C axis: M154/M155, G98/G99, the clamp, G12.1, G87'
Ui-StopApp
Write-Host 'PASS: live tools on a lathe in the CNC group'
