# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandMessage.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp tools/cli
# A failed preview's message opens at its first line (mitcad#1430) in the
# build VM, as tools/ui-panel-message-top-test.sh on Linux: the shared steps
# tools/ui-scripts/panel-message-top.mitcad-ui on the cube that mitcad-cli
# makes (tools/ui-scripts/panel-message-top.json), in a 1280 x 800 window,
# Docked and then Floating: Extrude1 edited, its distance 0 fails the
# preview, and the panel's error box shows its text from the first line,
# not scrolled down it. The box had opened scrolled in the Docked layout,
# its "Failed:" line cut off.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-panel-message-top-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-panel-message-top'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'panel-message-top.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.mitcad-ui'
foreach ($layout in @('docked', 'floating')) {
  Write-Host "--- $layout"
  Ui-StartApp "panel-message-top-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${layout}: the panel message driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the panel message driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Preview Extrude: failed' "${layout}: a distance of 0 fails the preview"
  Ui-ExpectLog 'Panel Extrude message box: error: ' "${layout}: its error in the panel's box"
  Ui-LogLines | Where-Object { $_ -match 'Panel Extrude message [0-9]+x[0-9]+' } | Select-Object -Last 1 |
    ForEach-Object { Write-Host "     $_" }
  Ui-ExpectLog 'TestDriver: done' "${layout}: the error box opens at its first line"
  Ui-StopApp
}
Write-Host "PASS: a failed preview's message opens at its first line"
