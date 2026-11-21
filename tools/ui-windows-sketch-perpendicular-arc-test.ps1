# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/EditTools.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchController.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui tools/ui-scripts/sketch-perpendicular-arc.json
# Perpendicular of a line and an arc (mitcad#2114) in the build VM, as
# tools/ui-sketch-perpendicular-arc-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-perpendicular-arc.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-perpendicular-arc.json):
# Shift+R, the line, then the arc add one perpendicular; the line turns
# through the arc's center, its symbol shows, and one undo step takes it
# away.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-perpendicular-arc-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-perpendicular-arc'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'perpendicular-arc.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-perpendicular-arc.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-perpendicular-arc.mitcad-ui'
Ui-StartApp 'sketch-perpendicular-arc' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch Perpendicular arc driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Perpendicular arc driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added constraint perpendicular k' 'Perpendicular took the line and the arc'
Ui-ExpectLog '(perpendicular by c3)' 'the symbol of the Perpendicular shows'
Ui-ExpectLog 'TestDriver: done' 'the line turned through the center, one undo step'
Ui-StopApp
Write-Host 'PASS: sketch Perpendicular of a line and an arc'
