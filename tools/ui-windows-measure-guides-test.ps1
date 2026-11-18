# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/framework/ResultRows.cpp app/framework/CommandSession.cpp app/framework/NoticePlaces.cpp app/MainWindowNoticeHover.cpp app/OcctViewer.cpp core/model-api/src/analysis.rs tools/cli
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-measure-guides'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plate.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'cli/tests/measure_guides.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Measure guides fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/measure-guides.mitcad-ui'
Ui-StartApp 'measure-guides' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'Measure guides driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Measure guides driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the minimum and the axis distance drawn apart, each row painting its segment'
Ui-StopApp
Write-Host 'PASS: Measure guides'
