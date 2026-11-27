# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/ViewController.cpp app/view/FaceProjection.hpp app/framework/TestDriver.cpp tools/ui-scripts/view-orthographic-faces.mitcad-ui
# Perspective with Orthographic Faces (mitcad#2473) in the build VM, as
# tools/ui-view-orthographic-faces-test.sh on Linux: the shared steps
# tools/ui-scripts/view-orthographic-faces.mitcad-ui on a new design. Chosen
# in View > Camera and saved; a click on the orientation cube's front face
# logs an orthographic camera, an orbit a perspective one with the same
# height at the target; View > Front is orthographic and the arrow keys turn
# it off the face and back; the cube's edge and corner stay perspective;
# Perspective ends the mode.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-orthographic-faces-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-orthographic-faces'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_LOG_ORIENTATION_CUBE = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/view-orthographic-faces.mitcad-ui'
Ui-StartApp 'view-orthographic-faces' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'orthographic faces driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "orthographic faces driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'perspective with orthographic faces)' 'the camera line names the mode'
Ui-ExpectLog 'Camera direction -0.577 0.577 -0.577 up' 'the cube corner click'
Ui-ExpectLog 'TestDriver: done' 'Perspective with Orthographic Faces'
Ui-StopApp
Write-Host 'PASS: perspective with orthographic faces'
