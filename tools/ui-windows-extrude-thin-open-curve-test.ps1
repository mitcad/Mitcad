# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli
# An open spline clicked in Extrude without changing Type (mitcad#2594) in
# the build VM, as tools/ui-extrude-thin-open-curve-test.sh on Linux: the
# shared steps tools/ui-scripts/extrude-thin-open-curve.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/extrude-thin-open-curve.json):
# the click on the spline sets Type to Thin Extrude and takes the spline
# alone, and OK adds the wall along it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-thin-open-curve-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-thin-open-curve'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'open-curve.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-thin-open-curve.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-thin-open-curve.mitcad-ui'
Ui-StartApp 'extrude-thin-open-curve' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'extrude thin open curve driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "extrude thin open curve driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Extrude Profiles: open sketch curve taken, Type set to Thin Extrude' 'the open spline sets Thin Extrude'
Ui-ExpectLog 'Added thin extrude (New Body), distance 10' 'the wall along the spline is added'
Ui-ExpectLog 'TestDriver: done' 'an open spline clicked in Extrude makes a thin extrusion'
Ui-StopApp
Write-Host 'PASS: open curve in Extrude sets Thin Extrude'
