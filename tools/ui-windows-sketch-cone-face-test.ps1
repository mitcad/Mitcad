# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindowSketch3d.cpp app/MainWindowVisibility.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp app/OcctViewer.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-cone-face.mitcad-ui tools/ui-scripts/sketch-cone-face.json
# A sketch on a cone's side (mitcad#1853) in the build VM, as
# tools/ui-sketch-cone-face-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-cone-face.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-cone-face.json, a cone of base radius 20
# and height 30): Create Sketch picks the cone's side and starts the
# sketch in the face's development, the layout grid not drawn on it; a
# rectangle typed 20 by 12 from its origin keeps its lengths on the face,
# its profile is drawn laid onto the face and its two dimensions' lines
# are drawn on the face; Finish Sketch ends the sketch and the grid is
# drawn again. The picture sketch-cone-face.png stays in <Out>\files.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-cone-face-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-cone-face'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cone.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-cone-face.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the cone was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-cone-face.mitcad-ui'
Ui-StartApp 'sketch-cone-face' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'sketch on a cone driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch on a cone driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch Sketch2 lies on its face in its developed coordinates (cone, radius 20.000 mm, half angle 33.690 deg)' "the cone's side picked in Create Sketch takes the sketch"
Ui-ExpectLog 'Layout grid not drawn: the sketch lies on a curved face' 'no layout grid on the curved face'
Ui-ExpectLog 'Added rectangle 20 x 12 mm at ' 'the rectangle typed 20 by 12 on the face'
Ui-ExpectLog 'laid onto its face' "the rectangle's profile drawn on the face"
Ui-ExpectLog ', 0 straight, ' "the dimensions' lines drawn on the face"
Ui-ExpectLog 'Layout grid drawn on its plane again' 'the grid drawn again after the sketch'
Ui-ExpectLog 'TestDriver: done' 'a sketch on a cone: picked, drawn by typed sizes, measured on the face, finished'
if (-not (Test-Path (Join-Path $work 'sketch-cone-face.png'))) { Ui-Fail 'the picture of the sketch on the cone was not saved' }
Ui-StopApp
Write-Host 'PASS: sketch on a conical face'
