# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PanelizeCommands.cpp core/model/src/features/panelize.rs core/model/src/document/panelize.rs tools/cli
# Panelize (mitcad#2005) in the build VM: the shared steps
# tools/ui-scripts/panelize.mitcad-ui (also run by tools/ui-panelize-test.sh
# and tools/ui-macos-panelize-test.sh) on the design mitcad-cli makes
# (tools/ui-scripts/panelize.json): a box made a cabinet of 18 mm panels in
# three steps (the sides, the back, the top and bottom with Remove
# Remainder), each panel's volume checked.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-panelize-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-panelize'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'panelize.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panelize.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/panelize.mitcad-ui'
Ui-StartApp 'panelize' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'panelize driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "panelize driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the cabinet made of five panels in three steps'
Ui-StopApp
Write-Host 'PASS: Panelize makes a cabinet of five panels in three steps'
