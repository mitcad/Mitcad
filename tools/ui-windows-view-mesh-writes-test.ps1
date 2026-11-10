# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/MainWindow.cpp app/framework/TestDriver.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/ModelShapes.cpp app/commands/ExtrudeCommands.cpp tools/cli
# The view never meshes the model's shapes itself (mitcad#1553, mitcad#1586)
# in the build VM, as tools/ui-view-mesh-writes-test.sh on Linux: the
# shared steps tools/ui-scripts/view-mesh-writes.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/view-mesh-writes.json): edges and
# a face picked in Fillet's input and at the idle point while the
# background evaluation runs, the body's edges checked for 3D polygons
# after each; then a construction plane, a sketch's profile and its curves,
# which the view meshes itself, drawn anew, hovered and picked while the
# background evaluation runs: drawn from shapes of the view's own, none of
# the body's, whose edges stay unmeshed.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-mesh-writes-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-mesh-writes'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'view-mesh-writes.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/view-mesh-writes.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/view-mesh-writes.mitcad-ui'
# Each background evaluation 6 s longer: the picks come while one runs
# (Diagnostics.hpp).
$env:MITCAD_TEST_RECOMPUTE_DELAY_MS = 'background evaluation=6000'
Ui-StartApp 'view-mesh-writes' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'view mesh writes driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "view mesh writes driver failed (exit code $exitCode)"
}
Ui-ExpectLog "TestDriver: view's own shapes: " 'the profiles, sketch curves and datum planes the view meshes are shapes of its own'
Ui-ExpectLog 'TestDriver: done' "picks, profiles and planes while the background evaluation runs mesh none of the body's edges"
Ui-StopApp
Write-Host 'PASS: the view meshes no model shape'
