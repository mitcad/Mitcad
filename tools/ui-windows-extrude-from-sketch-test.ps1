# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/commands/CommandSupport.cpp tools/cli
# Extrude (E) started from a zoomed sketch on a box's top face (mitcad#1227,
# mitcad#1226) in the build VM, as tools/ui-extrude-from-sketch-test.sh on
# Linux: the shared steps tools/ui-scripts/extrude-from-sketch.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/extrude-from-sketch.json):
# the sketch and the box inside the view's free area, a drawn circle as the
# automatic profile.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-from-sketch-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-from-sketch'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'extrude-from-sketch.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-from-sketch.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-from-sketch.mitcad-ui'
Ui-StartApp 'extrude-from-sketch' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Extrude from a sketch driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Extrude from a sketch driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Extrude from a zoomed sketch shows the sketch and its body, a drawn profile taken'
Ui-StopApp
Write-Host 'PASS: Extrude from a zoomed sketch'
