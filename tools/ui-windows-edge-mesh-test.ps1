# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/ViewEdges.cpp app/view/ViewEdges.hpp app/framework/TestDriver.cpp
# check-all sources: app/framework/ModelShapes.cpp app/sketch tools/cli
# The view's lines (mitcad#1633) in the build VM, as
# tools/ui-edge-mesh-test.sh on Linux: the shared steps
# tools/ui-scripts/edge-mesh.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/edge-mesh.json): a body's round edges drawn as fine
# polylines from detached copies of its edges, its own edges unmeshed, a
# highlight of its rounded edge on the body's polyline, and a sketch's
# line and arc 200 mm from the origin selected and zoomed in on, their
# highlights on the curves within a pixel.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-edge-mesh-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-edge-mesh'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'edge-mesh.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edge-mesh.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/edge-mesh.mitcad-ui'
Ui-StartApp 'edge-mesh' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'edge mesh driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "edge mesh driver failed (exit code $exitCode)"
}
Ui-ExpectLog ' edges drawn from copies, ' 'the body is drawn from fine copies of its edges'
Ui-ExpectLog ' sketch curve highlights, at most ' "the sketch curves' highlights lie on the curves"
Ui-ExpectLog 'TestDriver: done' "the body's lines are fine, highlights on them, its edges unmeshed"
Ui-StopApp
Write-Host 'PASS: the view draws its lines from fine edge copies'
