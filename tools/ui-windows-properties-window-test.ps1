# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/commands/PropertiesReport.cpp app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp
# check-all sources: core/model-api/src/analysis.rs core/model-api/src/physical_extent.rs tools/cli
# The Properties window (mitcad#1816): the shared input script
# tools/ui-scripts/properties-window.mitcad-ui of
# tools/ui-properties-window-test.sh, on the design mitcad-cli makes from
# tools/ui-scripts/properties-window.json.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-properties-window'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'properties-window.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/properties-window.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Properties window fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/properties-window.mitcad-ui'
Ui-StartApp 'properties-window' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Properties window driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Properties window driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'summary, merged rows, hover, details, Copy and one body'
Ui-StopApp
Write-Host 'PASS: Properties window'
