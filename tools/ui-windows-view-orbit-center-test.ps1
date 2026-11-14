# SPDX-License-Identifier: MIT
# check-all sources: app/view/OrbitCenterView.cpp app/OcctViewer.cpp app/MainWindow.cpp tools/cli
# The orbit's center (mitcad#1791, GitHub mitcad/Mitcad#300) in the build
# VM, as tools/ui-view-orbit-center-test.sh on Linux: the shared steps
# tools/ui-scripts/view-orbit-center.mitcad-ui on the block that mitcad-cli
# makes (tools/ui-scripts/view-orbit-center.json): Set Orbit Center Here
# near a corner of the block, an orbit that keeps that point's place,
# middle drags and clicks that leave it, Clear Orbit Center, and an orbit
# about the point under the cursor with Orbit center Cursor.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-orbit-center-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-orbit-center'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/view-orbit-center.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the block was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/view-orbit-center.mitcad-ui'
Ui-StartApp 'view-orbit-center' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'orbit center driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "orbit center driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the orbit turns about the fixed center and the point under the cursor'
Ui-StopApp
Write-Host 'PASS: the orbit turns about the fixed center and the point under the cursor'
