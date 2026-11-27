# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewPaintLayer.cpp app/view/ViewPaintLayer.hpp app/OcctViewer.cpp tools/cli
# check-all sources: tools/ui-scripts/paint-layer-fallback.mitcad-ui tools/ui-scripts/paint-layer-fallback.json
# The overlay layer's fallback (mitcad#2406) in the build VM, as
# tools/ui-paint-layer-fallback-test.sh on Linux (its gl-error run): with
# MITCAD_TEST_PAINT_LAYER_FAILURE=gl-error, the shared steps
# tools/ui-scripts/paint-layer-fallback.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/paint-layer-fallback.json): Sketch1
# edited, the log saying that the paint layers are widgets over the view
# for the rest of the session and why, the widget drawing the sketch, a line
# drawn with the Line tool painted by it, an orbit counting its paints.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-paint-layer-fallback-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-paint-layer-fallback'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'rectangle.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/paint-layer-fallback.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_PAINT_LAYER_FAILURE = 'gl-error'
$env:MITCAD_TEST_PAINT_LAYER_WHY = 'GL error GL_OUT_OF_MEMORY \(0x0505\).* after its texture upload \([0-9]+ x [0-9]+ RGBA\)'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/paint-layer-fallback.mitcad-ui'
Ui-StartApp 'paint-layer-fallback' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(170000)) { Ui-Fail 'paint layer fallback driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "paint layer fallback driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'View paint layers drawn as widgets over the view for the rest of the session: sketch overlay: ' 'the log says the paint layers are widgets over the view, and why'
Ui-ExpectLog "View paint layer 'sketch overlay' painted as a widget over the view: " 'the widget drew the sketch'
Ui-ExpectLog 'as a widget over the view' 'the orbit counted the widget''s paints'
Ui-ExpectLog 'TestDriver: done' 'the shared steps passed'
Ui-StopApp
Write-Host 'PASS: paint layer fallback'
