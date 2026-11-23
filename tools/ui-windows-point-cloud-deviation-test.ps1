# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudDeviation.cpp app/commands/InspectCommands.cpp app/MainWindowPointClouds.cpp
# check-all sources: app/view/PointCloudView.cpp app/view/PrintOverlay.cpp tools/ui-scripts/point-cloud-deviation.mitcad-ui
# check-all sources: app/view/PointClouds.cpp app/framework/TestDriverOrbitCenter.cpp
# Point clouds and Point Cloud Deviation (mitcad#2240) in the build VM, as
# tools/ui-point-cloud-deviation-test.sh on Linux: the shared steps
# tools/ui-scripts/point-cloud-deviation.mitcad-ui on the block mitcad-cli
# makes in the work folder (tools/ui-scripts/point-cloud-deviation.json) and
# the scan written here beside it (16 points 0.5 mm above the block's top,
# 16 points 0.3 mm above its bottom). Insert > Point Cloud reads the scan;
# Inspect > Point Cloud Deviation gives the statistics and colours the
# points, a point's tooltip its distance; the top face only, then 11 of the
# 32 points measured, the others dimmed (mitcad#2265); OK keeps it as
# Deviation1, saved (checked in the file here).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-point-cloud-deviation-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-point-cloud-deviation'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'deviation-part.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/point-cloud-deviation.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the block was not made: $($output -join ' / ')" }
# The scan: on a 4 x 4 grid, 0.5 mm above the top face and 0.3 mm above the
# bottom face.
$lines = New-Object System.Collections.Generic.List[string]
foreach ($x in 4, 8, 12, 16) {
  foreach ($y in 4, 8, 12, 16) {
    $lines.Add("$x $y 10.5")
    $lines.Add("$x $y 0.3")
  }
}
Set-Content -Path (Join-Path $work 'scan.xyz') -Value $lines -Encoding Ascii
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/point-cloud-deviation.mitcad-ui'
Ui-StartApp 'point-cloud-deviation' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'point cloud deviation driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "point cloud deviation driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Point clouds drawn: PointCloud1 32 points' 'the scan inserted and drawn'
Ui-ExpectLog '32 of 32 points, signed, mean 0.100 mm, mean absolute 0.400 mm' 'the statistics'
Ui-ExpectLog 'deviation 32 point(s)' 'the colour map drawn'
Ui-ExpectLog 'Deviation hover: Distance 9.700 mm (inside)' 'the tooltip at a point inside'
Ui-ExpectLog 'Point clouds drawn: PointCloud1 21 points not measured' 'the points not measured drawn dimmed'
Ui-ExpectLog 'Kept Point Cloud Deviation as Deviation1' 'kept in the document'
Ui-ExpectLog 'TestDriver: done' 'Point Cloud Deviation through the UI'
$saved = Get-Content -Raw $design
if (-not $saved.Contains('"point_clouds"') -or -not $saved.Contains('"type": "point_cloud_deviation"') -or
    -not $saved.Contains('"faces"')) {
  Ui-Fail 'no point cloud, deviation or its faces in the saved file'
}
Write-Host 'ok   the cloud and the analysis with its face are in the saved file'
Ui-StopApp
Write-Host 'PASS: Point Cloud Deviation is computed, coloured, kept and saved'
