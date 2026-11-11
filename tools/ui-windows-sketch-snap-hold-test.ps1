# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs
# check-all sources: core/model-api/src/sketch_clipboard.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-snap-hold.mitcad-ui tools/ui-scripts/sketch-snap-hold.json
# check-all sources: tools/ui-scripts/sketch-snap-hold-check.json
# Placements held where the snap marker showed (mitcad#1645, GitHub
# mitcad/Mitcad#245) in the build VM, as tools/ui-sketch-snap-hold-test.sh
# on Linux: the shared steps tools/ui-scripts/sketch-snap-hold.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/sketch-snap-hold.json):
# a circle pasted with its centre on a point, another moved by Move/Copy's
# handle onto a point, and a Point to Point copy onto a point are held
# there, and their centres follow the points in the saved design
# (tools/ui-scripts/sketch-snap-hold-check.json); a text's frame dragged
# between two points, a 3-Point Arc through a point and a Center Point Arc
# ending on a point are held there too.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-snap-hold-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-snap-hold'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'hold.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-snap-hold.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-snap-hold.mitcad-ui'
Ui-StartApp 'sketch-snap-hold' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'snap hold driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "snap hold driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Paste of p10 dropped on point p3' 'the pasted centre is held on the point'
Ui-ExpectLog 'Sketch handle holds p9 where it was dropped: point p5' "the handle's drop holds the centre"
Ui-ExpectLog ' held through point p5' "the 3-Point Arc's middle click holds it through the point"
Ui-ExpectLog 'TestDriver: done' 'pasted, moved, copied, framed and arc points held where they were dropped'
Ui-StopApp
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-snap-hold-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the saved holds do not keep the centres on their points' }
Write-Host 'PASS: sketch placements held where the snap marker showed'
