# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PointCloudTools.cpp app/commands/MeshShapeCommands.cpp app/commands/SketchNormal.cpp
# check-all sources: tools/ui-scripts/point-cloud-pairs-placed.mitcad-ui tools/ui-scripts/point-cloud-pairs-placed.json
# Point pairs on bodies of a placed component (mitcad#2566) in the build
# VM, as tools/ui-point-cloud-pairs-placed-test.sh on Linux: the shared
# steps tools/ui-scripts/point-cloud-pairs-placed.mitcad-ui on the design
# mitcad-cli makes in the work folder
# (tools/ui-scripts/point-cloud-pairs-placed.json): a block and a mesh block
# in a component whose occurrence is turned 90 degrees and moved, and a scan
# of the block's faces in the root. Align Point Cloud by three point pairs
# clicked on the placed block moves the scan onto the block as the view
# shows it within 0.01 mm RMS; Align Mesh by three pairs moves the mesh onto
# the block within 0.01 mm RMS.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-point-cloud-pairs-placed-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-point-cloud-pairs-placed'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'point-cloud-pairs-placed.mitcad'
Push-Location $work
try {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/point-cloud-pairs-placed.json') --save $design 2>&1
  $made = $LASTEXITCODE
} finally {
  Pop-Location
}
if ($made -ne 0) { Ui-Fail "mitcad-cli: the placed blocks and the scan were not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/point-cloud-pairs-placed.mitcad-ui'
Ui-StartApp 'point-cloud-pairs-placed' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'point cloud pairs placed driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "point cloud pairs placed driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Align Point Cloud target point: F1.b0 at ' 'a target point taken into the design'
Ui-ExpectLog 'Align Point Cloud points: 6' 'three point pairs picked on the placed block'
Ui-ExpectLog 'Aligned point cloud Scan to Body1: RMS 0.00' 'the cloud aligned within 0.01 mm'
Ui-ExpectLog 'Align Mesh points: 6' 'three point pairs picked on the placed mesh and block'
Ui-ExpectLog 'Added mesh align of F4.b0 onto F1.b0, 3 point pairs, best fit' 'the mesh aligned'
Ui-ExpectLog 'TestDriver: done' 'point pairs on a placed component through the UI'
Ui-StopApp
Write-Host 'PASS: point pairs on a placed component align a scan and a mesh onto its block'
