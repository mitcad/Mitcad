# SPDX-License-Identifier: MIT
# check-all sources: app/commands/CurvatureComb.cpp app/commands/CurvatureCombCommand.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchPalette.cpp
# check-all sources: tools/ui-scripts/curvature-comb.mitcad-ui
# Curvature Comb (mitcad#2520) in the build VM, as
# tools/ui-curvature-comb-test.sh on Linux: the shared steps
# tools/ui-scripts/curvature-comb.mitcad-ui on the design mitcad-cli makes in
# the work folder (tools/ui-scripts/curvature-comb.json). Inspect > Curvature
# Comb combs a cylinder's top edge; in Sketch2 the palette's Curvature comb
# shows an S-shaped spline's comb with its one inflection, read again while
# its end point is dragged.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-curvature-comb-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-curvature-comb'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'curvature-comb.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/curvature-comb.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the design was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/curvature-comb.mitcad-ui'
Ui-StartApp 'curvature-comb' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'curvature comb driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "curvature comb driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Inspect Curvature Comb: 1 curve(s), 24 teeth each, no inflection, minimum radius 10 mm' 'an edge combed'
Ui-ExpectLog 'Sketch curvature comb: c' 'a sketch curve combed in sketch mode'
Ui-ExpectLog 'Sketch curvature comb (dragging): c' 'the comb read again while dragged'
Ui-ExpectLog 'TestDriver: done' 'Curvature Comb through the UI'
Ui-StopApp
Write-Host 'PASS: Curvature Comb on an edge and in sketch mode'
