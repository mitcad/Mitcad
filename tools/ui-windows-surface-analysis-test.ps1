# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceAnalysis.cpp app/commands/InspectCommands.cpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp tools/ui-scripts/surface-analysis.mitcad-ui
# Surface quality analyses (mitcad#2517) in the build VM, as
# tools/ui-surface-analysis-test.sh on Linux: the shared steps
# tools/ui-scripts/surface-analysis.mitcad-ui on the cylinder mitcad-cli
# makes in the work folder (tools/ui-scripts/surface-analysis.json). Zebra,
# Curvature Map, Isocurve Analysis and Environment Map computed, drawn and
# kept, Zebra1 shown again and edited, saved (checked in the file here).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-surface-analysis-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-surface-analysis'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'surface-part.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/surface-analysis.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the cylinder was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/surface-analysis.mitcad-ui'
Ui-StartApp 'surface-analysis' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'surface analysis driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "surface analysis driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Inspect Zebra: direction (0, 0, 1), 3 stripes, width 0.3: Body1 dark' 'the zebra stripes'
Ui-ExpectLog 'Kept Zebra as Zebra1' 'the zebra kept'
Ui-ExpectLog 'Inspect Curvature Map: maximum 1/mm, limit 0.1 (automatic): Body1 min 0, max 0.1' 'the side 0.1/mm'
Ui-ExpectLog 'Inspect Isocurve Analysis: 2 per face, curvature colored: Body1 12 curves' 'the iso curves'
Ui-ExpectLog 'Inspect Environment Map: studio picture: Body1' 'the environment reflected'
Ui-ExpectLog 'Kept Environment Map as Environment1' 'the environment map kept'
Ui-ExpectLog 'TestDriver: done' 'the surface quality analyses through the UI'
$saved = Get-Content -Raw $design
foreach ($type in 'zebra', 'curvature', 'isocurve', 'environment') {
  if (-not $saved.Contains("""type"": ""$type""")) { Ui-Fail "no $type analysis in the saved file" }
}
Write-Host 'ok   the four analyses are in the saved file'
Ui-StopApp
Write-Host 'PASS: the surface quality analyses are computed, drawn, kept and saved'
