# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dTools.cpp app/sketch/Sketch3dShapes.cpp app/sketch/Sketch3dDraw.cpp app/sketch/Sketch3dSnaps.cpp app/sketch/Sketch3dController.cpp app/sketch/Sketch3dPalette.cpp app/MainWindowSketch3d.cpp app/framework/TestDriver.cpp tools/cli
# The free rotation of a 3D sketch's placement plane (mitcad#1661) in the
# build VM, as tools/ui-sketch3d-rotation-test.sh on Linux: the shared
# steps tools/ui-scripts/sketch3d-rotation.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch3d-rotation.json): a route drawn
# with the rotated plane. Rotate X 90 typed and Axis and Length 30 along the
# rotated Y put the end at (40, 0, 30); the Z handle dragged turns the
# plane by whole degrees and the next end stays in the sketch's XZ plane;
# a circle drawn on the rotated plane carries its frame; Reset, Show
# Handles and Show Plane; the sketch's stored frame is the same when it
# is opened again, and the plane is its XY.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch3d-rotation-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch3d-rotation'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sketch3d-rotation.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch3d-rotation.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch3d-rotation.mitcad-ui'
Ui-StartApp 'sketch3d-rotation' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '3D sketch rotation driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3D sketch rotation driver failed (exit code $exitCode)"
}
Ui-ExpectLog '3D sketch rotation handle X at ' 'the rotation handles were logged where they can be grabbed'
Ui-ExpectLog '3D sketch placement: along the rotated Y' 'the placement followed the rotated plane'
Ui-ExpectLog '"end":[40,0,30]' 'the end along the rotated Y with Axis and Length typed is (40, 0, 30)'
Ui-ExpectLog 'about Z by its handle' 'the Z handle''s drag rotated the plane'
Ui-ExpectLog '"plane":{"normal":' 'the circle on the rotated plane carried its frame'
Ui-ExpectLog '3D sketch plane reset: on the XY plane' 'Reset gave the sketch''s XY'
Ui-ExpectLog '3D sketch manipulator: rotation handles hidden' 'Show Handles hid the handles'
Ui-ExpectLog '3D sketch manipulator: plane hidden' 'Show Plane hid the plane'
Ui-ExpectLog 'TestDriver: done' 'a route was drawn with the rotated plane'
Ui-StopApp
Write-Host 'PASS: A route was drawn with the rotated plane'
