# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/resources/layout/default.json
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-mill-turn-sub.mitcad-ui tools/ui-scripts/cam-mill-turn-sub.json
# CAM mill-turn: live tools on the sub-spindle, the B head's and the
# sub-spindle's codes in Machines and Tools..., the sub-spindle's travel in
# the lathe check (mitcad#2375) on Windows, the shared input script
# tools/ui-scripts/cam-mill-turn-sub.mitcad-ui as
# tools/ui-cam-mill-turn-sub-test.sh runs it on Linux: Edit Machine... with
# the sub-spindle's page, a lathe setup handing over to the sub-spindle,
# the back faced, drilled and milled there; Generate; Simulate; Post
# Process... to the Fanuc turning dialect (the sub-spindle's stop, its C
# axis, G19, its axis E); the lathe check's finding; the sub-spindle's
# body, a zone moving with it, a control that mirrors only Z on it and an
# operation on the main spindle after the sub-spindle's (mitcad#2392); the
# sub-spindle's profile and the turret's body in Edit Machine..., the turret
# running into the sub-spindle's chuck, Simulate drawing the sub-spindle
# where it waits and at the grip (mitcad#2405).
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam-mill-turn-sub'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam-mill-turn-sub.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam-mill-turn-sub.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM mill-turn sub-spindle fixture creation failed' }

$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam-mill-turn-sub.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam-mill-turn-sub' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'CAM mill-turn sub-spindle driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM mill-turn sub-spindle driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "the sub-spindle's codes, live tools on the sub-spindle, generate, simulate, post and the lathe check"

# The Fanuc program with live tools on the sub-spindle.
$file = Join-Path $work 'setup1.nc'
if (-not (Test-Path $file)) { Ui-Fail 'no program written' }
$lines = @(Get-Content $file)
if ($lines[0] -ne '%') { Ui-Fail "the program does not start with the tape mark: $($lines[0])" }
foreach ($want in @('G55', 'M206', 'M264', 'G28 H0.', 'M114', 'G19', 'G18', 'M255', 'G99', '(DRILL 1)', '(FACING 1)',
    '(TURN FACING 2)')) {
  if ($lines -notcontains $want) { Ui-Fail "no '$want' in the program" }
}
if ($lines -contains 'M154') { Ui-Fail "the main spindle's C axis in the program" }
if (@($lines | Where-Object { $_ -match '^G1 E-?[0-9.]+ F[0-9.]+$' }).Count -eq 0) {
  Ui-Fail 'the sub-spindle does not come up to the part on its axis E'
}
# The sub-spindle stops before its C axis goes on for the hole.
$drill = [array]::IndexOf($lines, '(DRILL 1)')
$stop = -1
$on = -1
for ($i = $drill + 1; $i -lt $lines.Count; $i++) {
  if ($stop -lt 0 -and $lines[$i] -eq 'M206') { $stop = $i }
  if ($on -lt 0 -and $lines[$i] -eq 'M264') { $on = $i }
}
if ($drill -lt 0 -or $stop -lt 0 -or $on -lt 0 -or $stop -gt $on) {
  Ui-Fail 'the sub-spindle does not stop before its C axis goes on'
}
if ($lines[-2] -ne 'M30' -or $lines[-1] -ne '%') { Ui-Fail 'the program does not end with M30 and the tape mark' }
Write-Host 'ok   the Fanuc program with live tools on the sub-spindle: M206, M264, M114, G19, M255, E'
Ui-StopApp
Write-Host 'PASS: the sub-spindle codes, its live tools and its travel in the CNC group'
