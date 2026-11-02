# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/PickContext.cpp app/view/MeasurementOverlay.cpp app/commands/InspectCommands.cpp tools/cli
# check-all sources: app/MainWindowMake.cpp app/MainWindowMeasurements.cpp app/MainWindowAnnotations.cpp
# Measure on a face that carries an annotation (mitcad#1166), as
# tools/ui-measure-annotated-face-test.sh does on Linux: the same fixture
# and tools/ui-scripts/measure-annotated-face.mitcad-ui.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-measure-annotated-face'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'boss.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'cli/tests/measure_annotated_boss.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'annotated boss fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/measure-annotated-face.mitcad-ui'
Ui-StartApp 'measure-annotated-face' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'annotated face Measure driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "annotated face Measure driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Measure picks the annotated face, drawn opaque'
Ui-StopApp
Write-Host 'PASS: Measure on an annotated face'
