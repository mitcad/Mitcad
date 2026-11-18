# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/CameraHeight.hpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-resize.mitcad-ui tools/ui-scripts/sketch-resize.json
# The window resized while a sketch is edited (mitcad#1996) in the build VM,
# as tools/ui-sketch-resize-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-resize.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-resize.json): Edit Sketch1 in a wide
# window, the window made narrow and tall, Finish Sketch; then the other
# way. Each time the restored camera's aspect is the view's width over its
# height and a circle at its target is drawn round.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-resize-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-resize'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'resize.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-resize.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-resize.mitcad-ui'
Ui-StartApp 'sketch-resize' @('--ui=qt', '--no-mcp', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sketch resize driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch resize driver failed (exit code $exitCode)"
}
$restored = @(Ui-LogLines | Where-Object { $_ -match '^View: camera restored, .*\(the view.s aspect\), .*\(round\)$' }).Count
if ($restored -ne 2) { Ui-Fail "the view before the sketch was restored in the view's aspect $restored times, not twice" }
Write-Host "ok   restored twice in the view's aspect, a circle drawn round"
Ui-ExpectLog 'TestDriver: done' 'Finish Sketch after a resize restores the view in the window aspect'
Ui-StopApp
Write-Host 'PASS: sketch resize'
