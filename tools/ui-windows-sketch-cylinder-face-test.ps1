# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowSketch3d.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-cylinder-face.mitcad-ui tools/ui-scripts/sketch-cylinder-face.json
# A sketch on a cylinder's side (mitcad#1768) in the build VM, as
# tools/ui-sketch-cylinder-face-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-cylinder-face.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-cylinder-face.json, a cylinder
# of radius 20): Create Sketch picks the cylinder's side and starts the
# sketch in the face's developed coordinates; a rectangle typed 30 by 20
# from its origin is 30 mm around the face and 20 mm along it (each line's
# length on the face logged), its profile is drawn laid onto the face, and
# Finish Sketch ends the sketch.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-cylinder-face-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-cylinder-face'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cylinder.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-cylinder-face.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the cylinder was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-cylinder-face.mitcad-ui'
Ui-StartApp 'sketch-cylinder-face' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'sketch on a cylinder driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch on a cylinder driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch Sketch2 lies on its face in its developed coordinates (cylinder, radius 20.000 mm)' "the cylinder's side picked in Create Sketch takes the sketch"
Ui-ExpectLog 'Added rectangle 30 x 20 mm at ' 'the rectangle typed 30 by 20 on the face'
Ui-ExpectLog 'laid onto its face' "the rectangle's profile drawn on the face"
Ui-ExpectLog 'TestDriver: done' 'a sketch on a cylinder: picked, drawn by typed sizes, measured on the face, finished'
Ui-StopApp
Write-Host 'PASS: sketch on a cylindrical face'
