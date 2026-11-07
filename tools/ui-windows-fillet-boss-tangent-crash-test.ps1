# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionHover.cpp geometry/src/guard.cpp geometry/src/dressup.cpp tools/cli
# Fillet on the slot-and-boss part (mitcad#1429, GitHub mitcad/Mitcad#173)
# in the build VM: the steps tools/ui-scripts/fillet-slot-boss-1429.mitcad-ui
# on the part mitcad-cli makes (tools/ui-scripts/fillet-slot-boss-1429.json):
# Fillet on the plate's top edge that runs tangentially into the boss, its
# previews at several radii, Tangent Chain off and on, hovers, Enter and
# Escape. The application stays up whatever the fillet gives.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fillet-boss-tangent-crash-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fillet-boss-tangent-crash'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fillet-boss-tangent-crash.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/fillet-slot-boss-1429.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-slot-boss-1429.mitcad-ui'
Ui-StartApp 'fillet-boss-tangent-crash' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'fillet boss tangent driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "fillet boss tangent driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the fillet on the slot-and-boss part, its previews, hovers, Enter and Escape'
Ui-StopApp
Write-Host 'PASS: the fillet on the slot-and-boss part leaves the application up'
