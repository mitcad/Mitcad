# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudTools.cpp app/MainWindowPointClouds.cpp app/browser/BrowserController.cpp
# check-all sources: tools/ui-scripts/point-cloud-tools.mitcad-ui tools/ui-scripts/point-cloud-tools.json
# Point cloud tools (mitcad#2516) in the build VM, as
# tools/ui-point-cloud-tools-test.sh on Linux: the shared steps
# tools/ui-scripts/point-cloud-tools.mitcad-ui on the design mitcad-cli makes
# in the work folder (tools/ui-scripts/point-cloud-tools.json): a block and a
# scan of its faces, moved, with 3 outliers. Filter Point Cloud takes the
# outliers out; Align Point Cloud states the remaining deviation and moves
# the cloud onto the block; Reconstruct Mesh adds a closed mesh body of it;
# saved (checked in the file here).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-point-cloud-tools-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-point-cloud-tools'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'point-cloud-tools.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/point-cloud-tools.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the block and its scan were not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/point-cloud-tools.mitcad-ui'
Ui-StartApp 'point-cloud-tools' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'point cloud tools driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "point cloud tools driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Filtered point cloud Scan: outliers statistical' 'the outliers filtered out'
Ui-ExpectLog 'Point clouds drawn: Scan Filtered 110 points' 'the filtered cloud drawn'
Ui-ExpectLog 'Inspect Align Point Cloud: Scan Filtered to Body1: RMS 0.000 mm' 'the remaining deviation stated'
Ui-ExpectLog 'Aligned point cloud Scan Filtered to Body1' 'the cloud aligned'
Ui-ExpectLog 'Reconstructed a mesh of point cloud Scan Filtered' 'the mesh built'
Ui-ExpectLog 'TestDriver: done' 'the point cloud tools through the UI'
$saved = Get-Content -Raw $design
if (-not $saved.Contains('"Scan Filtered"') -or -not $saved.Contains('"source": "Scan Filtered"') -or
    -not $saved.Contains('"Scan Filtered Mesh"')) {
  Ui-Fail 'no filtered cloud or mesh of it in the saved file'
}
Write-Host 'ok   the filtered cloud and its mesh are in the saved file'
Ui-StopApp
Write-Host 'PASS: the point cloud tools filter, align and rebuild a scan'
