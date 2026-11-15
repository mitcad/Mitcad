# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/InspectCommands.cpp app/commands/PropertiesReport.cpp
# check-all sources: geometry/src/sweep.cpp geometry/src/taper_events.cpp tools/cli
# A widening taper that closes a slot (mitcad#1851): the shared input
# script tools/ui-scripts/taper-closes-slot.mitcad-ui of
# tools/ui-taper-closes-slot-test.sh, on the design mitcad-cli makes from
# tools/ui-scripts/taper-closes-slot.json.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-taper-closes-slot'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'taper-closes-slot.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/taper-closes-slot.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'taper closes slot fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/taper-closes-slot.mitcad-ui'
Ui-StartApp 'taper-closes-slot' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'taper closes slot driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "taper closes slot driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the tapered extrusion closing the slot added, its volume in Properties'
Ui-StopApp
Write-Host 'PASS: taper closes slot'
