# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-turning.mitcad-ui tools/ui-scripts/cam-turning.json
# CAM turning in the CNC group (mitcad#1869) on Windows, the shared input
# script tools/ui-scripts/cam-turning.mitcad-ui as tools/ui-cam-turning-test.sh
# runs it on Linux: a lathe setup; Turn Facing, Turn Roughing, Turn
# Finishing, Threading and Parting Off from their panels; Generate;
# Simulate (the turned section's summary); Post Process... to the Fanuc
# turning dialect (the program number, the modes, G50/G96/G97, G71, G70,
# G76, X as a diameter, the end).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam-turning'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam-turning.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam-turning.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM turning fixture creation failed' }

$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam-turning.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam-turning' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'CAM turning driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM turning driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a lathe setup, five turning operations, generate, simulate and post'

# The Fanuc turning program.
$file = Join-Path $work 'setup1.nc'
if (-not (Test-Path $file)) { Ui-Fail 'no program written' }
$lines = @(Get-Content $file)
if ($lines[0] -ne '%') { Ui-Fail "the program does not start with the tape mark: $($lines[0])" }
if ($lines[1] -ne 'O1000 (SETUP 1)') { Ui-Fail "the program's number is not first: $($lines[1])" }
foreach ($want in @('G21 G18 G40 G99', 'G50 S3000', 'G96 S200 M3')) {
  if ($lines -notcontains $want) { Ui-Fail "no '$want' in the program" }
}
foreach ($pattern in @('^G71 U[0-9.]+ R[0-9.]+$', '^G70 P[0-9]+ Q[0-9]+$', '^G97 S[1-9][0-9]* M3$', '^G76 P010060 ',
    '^G0 X48[.] Z')) {
  if (@($lines | Where-Object { $_ -match $pattern }).Count -eq 0) { Ui-Fail "no line like $pattern in the program" }
}
foreach ($name in @('TURN FACING 1', 'TURN ROUGHING 1', 'TURN FINISHING 1', 'THREADING 1', 'PARTING OFF 1')) {
  if ($lines -notcontains "($name)") { Ui-Fail "no $name in the program" }
}
if ($lines[-2] -ne 'M30' -or $lines[-1] -ne '%') { Ui-Fail 'the program does not end with M30 and the tape mark' }
Write-Host 'ok   the Fanuc turning program: number, modes, G50/G96/G97, G71, G70, G76, diameters'
Ui-StopApp
Write-Host 'PASS: turning in the CNC group'
