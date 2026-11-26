# SPDX-License-Identifier: MIT
# check-all sources: app/tutorials/GuideStep.cpp app/tutorials/TutorialGuide.cpp app/tutorials/MainWindowGuide.cpp app/sketch/SketchController.cpp app/sketch/SketchSnap.cpp app/MainWindow.cpp tools/ui-scripts/guide-sketch-input.mitcad-ui
# A tutorial's guide steps on the learner's sketch input (mitcad#2440) on
# Windows, as tools/ui-guide-sketch-input-test.sh does on Linux: the shared
# steps tools/ui-scripts/guide-sketch-input.mitcad-ui in an empty design (a
# sketch on XZ held back and one on XY taken; the 2-Point Rectangle's first
# corner off the origin held back and the one at the origin taken; the
# opposite corner without the width typed and Enter with another width
# held back, the width typed taken).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-guide-sketch-input-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-guide-sketch-input'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/guide-sketch-input.mitcad-ui'
Ui-StartApp 'guide-sketch-input' @('--ui=qt', '--no-mcp')
# The handle read while the app runs: without it ExitCode stays empty
# after the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Guide sketch input driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Guide sketch input driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Guide: step plane plane xz, not its XY' 'a sketch on another plane is held back'
Ui-ExpectLog 'Guide: step plane done: a sketch on XY' 'a sketch on the plane completes the plane step'
Ui-ExpectLog 'Guide: step corner done: a point at origin' 'the first corner at the origin completes the point step'
Ui-ExpectLog 'Guide: step width value 25 in width, not its 40' 'Enter with another width is held back'
Ui-ExpectLog 'Guide: step width done: 40 typed into width' 'the width typed completes the value step'
Ui-ExpectLog 'TestDriver: done' 'guide sketch input steps completed'
Ui-StopApp
Write-Host 'PASS: Guide steps on sketch input'
