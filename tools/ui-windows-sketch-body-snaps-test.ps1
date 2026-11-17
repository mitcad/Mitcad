# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-body-snaps.mitcad-ui tools/ui-scripts/sketch-body-snaps.json
# Every drawing tool snaps to body corners and edges in the sketch plane
# (mitcad#1967, GitHub mitcad/Mitcad#392) in the build VM, as
# tools/ui-sketch-body-snaps-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-body-snaps.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-body-snaps.json: a 60 x 40 x 20 block,
# Sketch2 on its top face): a line from a block corner and a rectangle's
# corner on another are held on the corners' linked projections
# (Coincident); a line started on a block edge away from its middle is held
# on the edge's linked projection.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-body-snaps-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-body-snaps'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-body-snaps.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-body-snaps.mitcad-ui'
Ui-StartApp 'sketch-body-snaps' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch body snaps driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch body snaps driver failed (exit code $exitCode)"
}
Ui-ExpectLog ', the projected corner V{' 'a drawn point held on a projected block corner'
Ui-ExpectLog ', the projected edge E{' 'a line start held on a projected block edge'
Ui-ExpectLog 'TestDriver: done' 'a line and a rectangle from block corners, a line from a block edge'
Ui-StopApp
Write-Host 'PASS: sketch body snaps'
