# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudTools.cpp app/MainWindowPointClouds.cpp app/framework/CommandSession.cpp
# check-all sources: tools/ui-scripts/point-cloud-pairs.mitcad-ui tools/ui-scripts/point-cloud-pairs.json
# Point pairs and the crop box of the point cloud tools (mitcad#2565) in the
# build VM, as tools/ui-point-cloud-pairs-test.sh on Linux: the shared steps
# tools/ui-scripts/point-cloud-pairs.mitcad-ui on the design mitcad-cli makes
# in the work folder (tools/ui-scripts/point-cloud-pairs.json): a block and a
# scan of its faces turned 30 degrees and moved 20 mm. Align Point Cloud by
# three point pairs clicked in the view and the best fit moves the scan back
# onto the block within 0.01 mm RMS; Filter Point Cloud with a crop box
# keeps the 140 points inside it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-point-cloud-pairs-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-point-cloud-pairs'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'point-cloud-pairs.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/point-cloud-pairs.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the block and its scan were not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/point-cloud-pairs.mitcad-ui'
Ui-StartApp 'point-cloud-pairs' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'point cloud pairs driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "point cloud pairs driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Align Point Cloud points: 6' 'three point pairs picked in the view'
Ui-ExpectLog 'Align Point Cloud: Backspace taken' 'a picked point taken back'
Ui-ExpectLog 'Inspect Align Point Cloud: Scan to Body1: RMS 0.00' 'the remaining deviation stated'
Ui-ExpectLog 'Aligned point cloud Scan to Body1: RMS 0.00' 'the cloud aligned within 0.01 mm'
Ui-ExpectLog 'cropped by a box from -1.000,-1.000,-1.000 to 9.000,21.000,11.000, inside kept' 'the box crop'
Ui-ExpectLog 'Point clouds drawn: Scan Filtered 140 points' 'the points inside the box kept'
Ui-ExpectLog 'TestDriver: done' 'point pairs and the crop box through the UI'
Ui-StopApp
Write-Host 'PASS: point pairs align a scan and a crop box keeps the points inside it'
