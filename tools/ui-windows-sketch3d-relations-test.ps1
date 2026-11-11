# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dPalette.cpp app/sketch/Sketch3dModel.cpp app/sketch/Sketch3dEdit.cpp app/sketch/Sketch3dController.cpp app/sketch/Sketch3dEditing.cpp app/MainWindowSketch3d.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# 3D relations applied from the 3D Sketch Palette (mitcad#1661) in the
# build VM, as tools/ui-sketch3d-relations-test.sh on Linux: the shared
# steps tools/ui-scripts/sketch3d-relations.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch3d-relations.json): Parallel and
# Perpendicular of lines in space, Angle of two lines (a driving angle
# listed in degrees), Symmetric of two points about the sketch's YZ plane
# picked in the palette; each one edit, listed in words.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch3d-relations-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch3d-relations'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sketch3d-relations.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch3d-relations.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch3d-relations.mitcad-ui'
Ui-StartApp 'sketch3d-relations' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '3D sketch relations driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3D sketch relations driver failed (exit code $exitCode)"
}
Ui-ExpectLog '3D sketch edit parallel: done' 'Parallel of two lines in space was applied'
Ui-ExpectLog '3D sketch edit perpendicular: done' 'Perpendicular of two lines was applied'
Ui-ExpectLog '3D sketch edit angle_relation: done' 'the two lines were held at their angle'
Ui-ExpectLog '| shown: Angle 1 on Line 2, Line 4: ' 'the angle is listed in words, in degrees'
Ui-ExpectLog '3D sketch edit symmetric about {"frame":"x"}: done' 'Symmetric of two points about the YZ plane'
Ui-ExpectLog '| shown: Symmetric 1 on Point 9, Point 10' 'the symmetry is listed in words'
Ui-ExpectLog 'TestDriver: done' 'the relations were applied from the palette'
Ui-StopApp
Write-Host 'PASS: 3D relations are applied from the 3D Sketch Palette'
