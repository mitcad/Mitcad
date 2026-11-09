# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchSnap.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-drag-join.mitcad-ui tools/ui-scripts/sketch-drag-join.json
# A dragged sketch point snaps to points and curves and is held where it is
# dropped (mitcad#1538) in the build VM, as tools/ui-sketch-drag-join-test.sh
# on Linux: the shared steps tools/ui-scripts/sketch-drag-join.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/sketch-drag-join.json):
# an open polyline drawn with the Line tool, its last point dragged onto
# its first with Shift held (not held there) and without (held there, the
# profile closed, one undo step with the drag).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-drag-join-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-drag-join'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'join.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-drag-join.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-drag-join.mitcad-ui'
Ui-StartApp 'sketch-drag-join' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'drag join driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "drag join driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'off (Shift or Alt)' 'Shift turns the drag snap off'
Ui-ExpectLog ': held there' 'the point dropped on the first point is held there'
Ui-ExpectLog 'TestDriver: done' 'a dragged point snaps and is held where it is dropped, not with Shift'
Ui-StopApp
Write-Host 'PASS: sketch drag snaps and holds the dropped point'
