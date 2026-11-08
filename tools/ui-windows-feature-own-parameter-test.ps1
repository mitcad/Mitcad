# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/ParameterField.cpp app/MainWindow.cpp
# check-all sources: core/model/src/document.rs core/model-api/src/lib.rs tools/cli
# check-all sources: tools/ui-scripts/feature-own-parameter.mitcad-ui tools/ui-scripts/feature-own-parameter.json
# A command panel value that uses the field's own parameter keeps that
# parameter as a user parameter (mitcad#1536) in the build VM, as
# tools/ui-feature-own-parameter-test.sh on Linux: the shared steps
# tools/ui-scripts/feature-own-parameter.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/feature-own-parameter.json): `dep` in
# Edit Extrude's Distance lists depth as kept as a user parameter, and
# depth * 2 gives the distance a new parameter and leaves depth a user
# parameter at 10 mm.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-feature-own-parameter-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-feature-own-parameter'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'own.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/feature-own-parameter.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/feature-own-parameter.mitcad-ui'
Ui-StartApp 'feature-own-parameter' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'own parameter driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "own parameter driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Extrude: ok, info: depth is now a user parameter (10 mm)' 'the panel says what OK will move'
Ui-ExpectLog 'Feature notice: depth is now a user parameter (10 mm)' 'the status bar says what moved'
Ui-ExpectLog 'TestDriver: done' 'the own parameter kept as a user parameter by a panel value'
Ui-StopApp
Write-Host 'PASS: feature value own parameter kept as a user parameter'
