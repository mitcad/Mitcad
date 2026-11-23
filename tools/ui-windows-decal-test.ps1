# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowDecals.cpp app/view/DecalView.cpp app/view/Decals.cpp app/browser app/framework/TestDriver.cpp tools/cli tools/ui-scripts/decal.mitcad-ui tools/ui-scripts/decal.json
# Decals (mitcad#2244) in the build VM, as tools/ui-decal-test.sh on Linux:
# the shared steps tools/ui-scripts/decal.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/decal.json) and a PNG file written
# here. A decal put on a box's top and on a rod's side, turned and flipped,
# checked in the log and by the colour of the view's pixels on the faces,
# and hidden behind another body.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-decal-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-decal'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
# A PNG file of 16 x 8 pixels: the left 12 columns opaque red, the right 4
# transparent (the same bytes as the Linux and macOS tests write).
$png = 'iVBORw0KGgoAAAANSUhEUgAAABAAAAAICAYAAADwdn+XAAAAF0lEQVR42mP4z8DwnxTMgA5GDRgOBgAARMC/QQD+4HYAAAAASUVORK5CYII='
[System.IO.File]::WriteAllBytes((Join-Path $work 'decal.png'), [System.Convert]::FromBase64String($png))
$design = Join-Path $work 'decal.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/decal.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/decal.mitcad-ui'
Ui-StartApp 'decal' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'decal driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "decal driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Decals drawn: Decal1 20.0 x 10.0 mm on a plane turned 90 deg flipped 100%' 'the decal turned and flipped'
Ui-ExpectLog 'Decal2 15.0 x 7.5 mm on a cylinder 100%' 'the decal on the rod'
Ui-ExpectLog 'TestDriver: done' 'Decals (mitcad#2244): every step of the script'
Ui-StopApp
Write-Host 'PASS: Decals (mitcad#2244)'
