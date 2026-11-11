# SPDX-License-Identifier: MIT
# The Start page's rounded backdrop (mitcad#1439) in the build VM's
# session 0, a one-off visual check, not a ctest: the steps
# tools/ui-scripts/start-backdrop-look.mitcad-ui on the block that
# mitcad-cli makes (tools/ui-scripts/start-page-delete.json), in the
# Floating and the Docked layout, as tools/ui-start-backdrop-look.sh on
# Linux. The pictures go to -Out:
#   start-backdrop-floating-light.png  start-backdrop-floating-dark.png
#   start-backdrop-docked-light.png    start-backdrop-docked-dark.png
# with the app's logs beside them.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-start-backdrop-look.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-start-backdrop-look'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$pictures = $script:UiOut
$design = Join-Path $pictures 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/start-page-delete.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'block.mitcad was not made' }
$env:MITCAD_TEST_WORK = $pictures
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/start-backdrop-look.mitcad-ui'

foreach ($layout in 'floating', 'docked') {
  $env:MITCAD_LOOK_LAYOUT = $layout
  Ui-StartApp "start-backdrop-look-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${layout}: the look driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the look driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${layout}: the pictures were taken"
  Ui-StopApp
}
Get-ChildItem -Path $pictures -Filter 'start-backdrop-*.png' | ForEach-Object { Write-Host "ok   $($_.FullName)" }
