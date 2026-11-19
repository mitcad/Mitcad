# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp app/commands/SheetCommands.cpp core/model/src/features/sheet.rs core/model/src/sheet app/framework/TestDriver.cpp tools/cli tools/ui-scripts/kerf-bend.mitcad-ui tools/ui-scripts/kerf-bend.json
# Kerf Bend (mitcad#1939) in the build VM, as tools/ui-kerf-bend-test.sh on Linux: the
# shared steps tools/ui-scripts/kerf-bend.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/kerf-bend.json). A kerf bend edited from the timeline opens in Kerf Bend with its slit lattice, its row pitch changed.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-kerf-bend-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-kerf-bend'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'kerf-bend.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/kerf-bend.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/kerf-bend.mitcad-ui'
Ui-StartApp 'kerf-bend' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'kerf-bend driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "kerf-bend driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Editing F5 with Kerf Bend' 'the kerf bend opened in its own panel'
Ui-ExpectLog 'Edited F5' 'the kerf bend was edited'
Ui-ExpectLog 'TestDriver: done' 'Kerf Bend (mitcad#1939): every step of the script'
Ui-StopApp
Write-Host 'PASS: Kerf Bend (mitcad#1939)'
