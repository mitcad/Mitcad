# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/MainWindowSketchCancel.cpp app/MainWindowSketch3d.cpp
# check-all sources: app/framework/DocumentSession.hpp app/sketch/SketchPalette.cpp app/sketch/Sketch3dPalette.cpp
# check-all sources: app/sketch/SketchController.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-cancel.mitcad-ui tools/ui-scripts/sketch-cancel.json
# check-all sources: app/sketch/Sketch3dController.cpp app/sketch/Sketch3dTools.cpp
# Cancel Sketch, and every sketch edit as one undo step at Finish Sketch
# (mitcad#2225) in the build VM, as tools/ui-sketch-cancel-test.sh on
# Linux: the shared steps tools/ui-scripts/sketch-cancel.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/sketch-cancel.json): Edit
# Sketch on a sketch with a feature after it, a line and a dimension,
# Cancel Sketch under the palette: the sketch, the later feature and the
# timeline marker as before, Redo brings the edit back in one step; Create
# Sketch, a line, Cancel Sketch from the context menu: no new sketch; the
# same Cancel in a 3D sketch; Cancel with nothing changed leaves as Finish
# Sketch; a 3D sketch's edit is one undo step at Finish Sketch. Redo after
# Cancel Sketch leaves the timeline marker where Finish Sketch leaves it,
# and the app quits with a 3D sketch's Line tool running, without a crash
# (mitcad#2263).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-cancel-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-cancel'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cancel.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-cancel.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-cancel.mitcad-ui'
Ui-StartApp 'sketch-cancel' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sketch Cancel driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Cancel driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch cancelled: Sketch1, ' 'Edit Sketch, a line and a dimension, Cancel Sketch: taken back'
Ui-ExpectLog 'Redo: Edit Sketch1' 'Redo brings the line and the dimension back in one step'
Ui-ExpectLog ', taken back, it was new' 'Create Sketch, a line, Cancel Sketch: no new sketch'
Ui-ExpectLog 'Sketch cancelled: 3DSketch1, ' 'the same Cancel in a 3D sketch'
Ui-ExpectLog 'Sketch cancelled: Sketch1, 0 step(s) taken back' 'nothing changed: left as Finish Sketch leaves it'
Ui-ExpectLog '3D sketch tool Line: ends with the window' 'the app quits with the 3D Line tool running (mitcad#2263)'
Ui-ExpectLog 'TestDriver: done' 'Cancel Sketch takes the sketch edit back, Finish Sketch makes it one step'
Ui-StopApp
Write-Host 'PASS: sketch Cancel'
