# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_silhouettes.rs
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src/compose.rs
# check-all sources: tools/ui-scripts/drawing-curved-outline.mitcad-ui tools/ui-scripts/drawing-curved-outline.json
# Dimensions to a curved outline's top and a section's label dragged
# (mitcad#1909) in the build VM, as tools/ui-drawing-curved-outline-test.sh
# on Linux: the shared steps tools/ui-scripts/drawing-curved-outline.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/drawing-curved-outline.json): Dimension from a pipe
# bend's end to the top of its outer outline reads 70; the section's label
# dragged keeps its offset, and Reset Label Position puts it back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-curved-outline-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-curved-outline'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'bend.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-curved-outline.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-curved-outline.mitcad-ui'
Ui-StartApp 'drawing-curved-outline' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'curved outline driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "curved outline driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Drawing picked point (2): ' 'the top of the outer outline picked as the second pick'
Ui-ExpectLog 'Drawing view 2 label dragged by ' 'the section label dragged'
Ui-ExpectLog 'TestDriver: done' 'the overall height read 70, the label moved and put back'
Ui-StopApp
Write-Host 'PASS: drawing dimension to a curved outline and a dragged section label'
