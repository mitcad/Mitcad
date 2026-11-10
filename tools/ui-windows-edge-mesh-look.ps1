# SPDX-License-Identifier: MIT
# Pictures of the view's lines zoomed in (mitcad#1633) in the build VM's
# session 0, a one-off visual check, not a ctest: the steps
# tools/ui-scripts/edge-mesh-look.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/edge-mesh.json), with only test-driver steps
# older builds have too, so that the same script gives the pictures before
# and after the fix. The pictures (listed in the input script) and the
# design go to -Out; the app's log is in -Out as the other Windows UI
# scripts leave it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-edge-mesh-look.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe -Out dir [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-edge-mesh-look'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$pictures = $script:UiOut
$design = Join-Path $pictures 'edge-mesh.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/edge-mesh.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $pictures
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/edge-mesh-look.mitcad-ui'
Ui-StartApp 'edge-mesh-look' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'edge mesh look driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "edge mesh look driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the pictures were taken'
Ui-StopApp
Get-ChildItem -Path $pictures -Filter 'edge-mesh-*.png' | ForEach-Object { Write-Host "ok   $($_.FullName)" }
