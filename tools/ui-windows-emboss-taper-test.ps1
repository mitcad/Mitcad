# SPDX-License-Identifier: MIT
# check-all sources: app/commands/EmbossCommands.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/emboss-taper.mitcad-ui tools/ui-scripts/emboss-taper.json
# Emboss's Taper Angle (mitcad#1547) in the build VM, as
# tools/ui-emboss-taper-test.sh on Linux: the shared steps
# tools/ui-scripts/emboss-taper.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/emboss-taper.json): a square wrapped onto a rod
# 45 deep with Taper Angle 10 typed in the panel, previewed, and OK logs
# the taper and grows the rod by the tapered emboss.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-emboss-taper-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-emboss-taper'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'taper.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/emboss-taper.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/emboss-taper.mitcad-ui'
Ui-StartApp 'emboss-taper' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'emboss taper driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "emboss taper driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added emboss (Raise) of 1 profile(s) on a cylindrical face, wrapped, depth 45, taper 10 deg' 'the tapered emboss added'
Ui-ExpectLog 'TestDriver: done' 'Taper Angle set in the panel, previewed and kept by OK'
Ui-StopApp
Write-Host 'PASS: emboss taper angle'
