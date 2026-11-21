# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/ViewController.cpp app/framework/FloatingLayout.cpp app/framework/TestDriver.cpp tools/ui-scripts/cube-size.mitcad-ui
# The orientation cube's size (mitcad#2071, GitHub mitcad/Mitcad#447) in
# the build VM, as tools/ui-cube-size-test.sh on Linux: the shared steps
# tools/ui-scripts/cube-size.mitcad-ui on a new design, Floating. 75 % and
# 150 % from the cube's right-click menu and from Preferences > Display; the
# cube, its Home button and its arrows follow the size, no two buttons
# overlap at 75 %, a click on the cube's top face still turns the view to
# Top, and the Command card on the upper right moves below the larger cube.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-cube-size-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cube-size'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_LOG_ORIENTATION_CUBE = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cube-size.mitcad-ui'
Ui-StartApp 'cube-size' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'cube size driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "cube size driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Orientation cube corner 75 %: area 143, offset 75' 'the corner at 75 %'
Ui-ExpectLog 'Orientation cube corner 150 %: area 285, offset 150' 'the corner at 150 %'
Ui-ExpectLog 'TestDriver: clear of the cube: commandDock' 'the Command card below the cube'
Ui-ExpectLog 'Preferences: orientation cube size 150 %' 'Preferences saved the size'
Ui-ExpectLog 'TestDriver: done' 'the orientation cube size'
Ui-StopApp
Write-Host 'PASS: orientation cube size'
