# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/Ribbon.cpp app/framework/Ribbon.hpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp app/sketch/DrawTools.cpp
# check-all sources: tools/ui-scripts/sketch-point-top-level.mitcad-ui
# The sketch Point at the top level of the Create menu, after Lines, and as
# a button of its own in the ribbon's Create group (mitcad#1545, GitHub
# mitcad/Mitcad#200) on Windows, the shared input script
# tools/ui-scripts/sketch-point-top-level.mitcad-ui as
# tools/ui-sketch-point-top-level-test.sh runs it on Linux: the Create
# menu's entries with Point second and not in Other Geometry, Point chosen
# there adds a point, and the ribbon's Point button adds another.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-point-top-level-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-point-top-level'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-point-top-level.mitcad-ui'
Ui-StartApp 'sketch-point-top-level' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'sketch Point driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Point driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: popup entries: Lines | Point | Rectangles | ' 'Point at the top level of the Create menu'
Ui-ExpectLog 'Ribbon button sketch.point at ' 'a Point button in the Create group'
Ui-ExpectLog 'TestDriver: done' 'Point from the Create menu and from its ribbon button'
Ui-StopApp
Write-Host 'PASS: sketch Point at the top level'
