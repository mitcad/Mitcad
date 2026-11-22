# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/AutoConstraint.cpp app/sketch/SketchOverlay.cpp
# check-all sources: app/sketch/SketchController.cpp app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui tools/ui-scripts/sketch-horizontal-vertical-other.json
# Horizontal/Vertical takes the other direction when only that can be
# added (mitcad#2184) in the build VM, as
# tools/ui-sketch-horizontal-vertical-other-test.sh on Linux: the shared
# steps tools/ui-scripts/sketch-horizontal-vertical-other.mitcad-ui on the
# design that mitcad-cli makes
# (tools/ui-scripts/sketch-horizontal-vertical-other.json): two points
# nearer to level whose heights are held; Shift+V, the first point, the
# pointer on the second shows the plumb glyph, and the click adds a
# vertical alignment instead of refusing; one undo step takes it away.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-horizontal-vertical-other-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-horizontal-vertical-other'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'horizontal-vertical-other.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-horizontal-vertical-other.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-horizontal-vertical-other.mitcad-ui'
Ui-StartApp 'sketch-horizontal-vertical-other' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch Horizontal/Vertical other direction driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Horizontal/Vertical other direction driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Horizontal/Vertical would add vertical_points on p1, p2' 'the hover glyph shows plumb'
Ui-ExpectLog 'Added constraint vertical_points k' 'Horizontal/Vertical added plumb instead of refusing'
Ui-ExpectLog 'TestDriver: done' 'p2 slid under p1, one undo step'
Ui-StopApp
Write-Host 'PASS: sketch Horizontal/Vertical in the other direction'
