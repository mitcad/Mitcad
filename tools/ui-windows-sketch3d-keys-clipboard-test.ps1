# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dClipboard.cpp app/sketch/SketchClipboard.cpp app/sketch/Sketch3dTools.cpp app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dController.cpp app/MainWindowSketch3d.cpp app/framework/TestDriver.cpp core/sketch/src/spatial_clipboard.rs core/model-api/src/sketch3d.rs core/model-api/src/sketch_clipboard.rs tools/cli
# The 3D sketch manipulator's keys and Copy and Paste between 3D sketches
# (mitcad#1661) in the build VM, as tools/ui-sketch3d-keys-clipboard-test.sh
# on Linux: the shared steps tools/ui-scripts/sketch3d-keys-clipboard.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/sketch3d-keys-clipboard.json): with the Line tool, Tab
# and Shift+Tab cycle the placement plane and Up Arrow switches to Normal
# and back, each logged with its key; in the fields Tab moves between them;
# the route rises to z 15; copied from 3DSketch1 and pasted into 3DSketch2
# it keeps its z, its relations and dimensions, as one undo step (session
# 0 has no writable clipboard: the application keeps the copy).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch3d-keys-clipboard-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch3d-keys-clipboard'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sketch3d-keys-clipboard.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch3d-keys-clipboard.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch3d-keys-clipboard.mitcad-ui'
Ui-StartApp 'sketch3d-keys-clipboard' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '3D sketch keys and clipboard driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3D sketch keys and clipboard driver failed (exit code $exitCode)"
}
Ui-ExpectLog '3D sketch control keys: planes Tab: the next plane, Shift+Tab: the previous plane' 'the plane controls'' tooltips name Tab and Shift+Tab'
Ui-ExpectLog '3D placement plane: YZ (Tab)' 'Tab changed the plane'
Ui-ExpectLog '3D placement plane: YZ (Shift+Tab)' 'Shift+Tab changed the plane back'
Ui-ExpectLog '3D placement: Normal (Up)' 'Up Arrow switched to Normal'
Ui-ExpectLog '3D placement: On Plane (Up)' 'Up Arrow switched back to On Plane'
Ui-ExpectLog '3D sketch field focus: dy' 'Tab in a field moved to the next field'
Ui-ExpectLog '3D sketch anchor at p2 (10.000, 0.000, 15.000)' 'the route rose to z 15'
Ui-ExpectLog '3D sketch copied c3 from 3DSketch1: ' 'the line was copied'
Ui-ExpectLog '3D sketch Pasted p1, p2, c3 into 3DSketch2: ' 'it was pasted into the other 3D sketch'
Ui-ExpectLog 'Point p2 at 10.00, 0.00, 15.00 mm' 'the pasted end kept its z'
Ui-ExpectLog 'Undo: Paste in 3DSketch2' 'the paste was one undo step'
Ui-ExpectLog 'TestDriver: done' 'the keys and the clipboard work in a 3D sketch'
Ui-StopApp
Write-Host 'PASS: The 3D sketch keys and clipboard work'
