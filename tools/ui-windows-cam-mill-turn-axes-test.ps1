# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn-axes.mitcad-ui tools/ui-scripts/cam-mill-turn-axes.json
# CAM mill-turn with a Y axis, a B head and a sub-spindle (mitcad#2319) on
# Windows, the shared input script tools/ui-scripts/cam-mill-turn-axes.mitcad-ui
# as tools/ui-cam-mill-turn-axes-test.sh runs it on Linux: a lathe setup
# handing over to the sub-spindle, a flat with the Y axis, a hole on a
# tilted plane, a sketch wrapped onto the diameter, the part-off and the
# back on the sub-spindle; Generate; Simulate (the stock in 3D); Post
# Process... to the Fanuc turning dialect (G19, the tilted working plane,
# G7.1, the hand-over, G55, the sub-spindle's spindle).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam-mill-turn-axes'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam-mill-turn-axes.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam-mill-turn-axes.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM mill-turn axes fixture creation failed' }

$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam-mill-turn-axes.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam-mill-turn-axes' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(400000)) { Ui-Fail 'CAM mill-turn axes driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM mill-turn axes driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a lathe setup with a hand-over, Y, B and wrapped live tools, generate, simulate and post'

# The Fanuc program with the Y axis, the B head and the sub-spindle.
$file = Join-Path $work 'setup1.nc'
if (-not (Test-Path $file)) { Ui-Fail 'no program written' }
$lines = @(Get-Content $file)
if ($lines[0] -ne '%') { Ui-Fail "the program does not start with the tape mark: $($lines[0])" }
foreach ($want in @('M154', 'G98', 'G0 C0.', 'M14', 'G19', 'G18', 'G68.2 X0. Y0. Z0. I90. J45. K-90.', 'G53.1', 'M107',
    'G69', 'G7.1 C15.', 'G7.1 C0.', 'M213', 'M79', 'M78', 'M212', 'G55', 'M155', 'G99')) {
  if ($lines -notcontains $want) { Ui-Fail "no '$want' in the program" }
}
foreach ($pattern in @('^G83 X-?[0-9.]+ Y-?[0-9.]+ Z-?[0-9.]+ R-?[0-9.]+( Q[0-9]+)? F[0-9.]+$',
    '^G1 A-?[0-9.]+ F[0-9.]+$', '^G96 S[0-9]+ M203$')) {
  if (@($lines | Where-Object { $_ -match $pattern }).Count -eq 0) { Ui-Fail "no line like $pattern in the program" }
}
foreach ($name in @('TURN FACING 1', 'FACING 1', 'DRILL 1', 'CONTOUR 1', 'PARTING OFF 1', 'TURN FACING 2',
    'HAND-OVER TO THE SUB-SPINDLE')) {
  if ($lines -notcontains "($name)") { Ui-Fail "no $name in the program" }
}
if ($lines[-2] -ne 'M30' -or $lines[-1] -ne '%') { Ui-Fail 'the program does not end with M30 and the tape mark' }
Write-Host 'ok   the Fanuc program with Y, B and the sub-spindle: G19, G68.2, G7.1, the hand-over, G55, M203'
Ui-StopApp
Write-Host 'PASS: Y, B, wrapped sketches and the sub-spindle on a lathe in the CNC group'
