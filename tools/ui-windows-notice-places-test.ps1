# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp app/framework/CommandMessage.cpp app/framework/MessageBand.cpp app/framework/TestDriver.cpp geometry/src/dressup_check.cpp tools/cli
# Notice places (mitcad#1327) in the build VM, as
# tools/ui-notice-places-test.sh on Linux: the shared steps
# tools/ui-scripts/notice-places.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/notice-places.json): a rim's rounding that runs
# onto the rounding of the block's front edge warns in plain words, and the pointer over the
# warning paints its places in the view on the Timeline item, in the
# message band and in the command panel's message while it is edited;
# moving away clears them. The screenshots (notice-places-*.png, in the
# output folder's shots) show the highlight.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-notice-places-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-notice-places'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
$shots = Join-Path $script:UiOut 'shots'
New-Item -ItemType Directory -Force $work | Out-Null
New-Item -ItemType Directory -Force $shots | Out-Null
$design = Join-Path $work 'notice-places.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/notice-places.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_SHOTS = $shots
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/notice-places.mitcad-ui'
Ui-StartApp 'notice-places' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'notice places driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "notice places driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Notice hover highlighted: timeline: ' 'the Timeline item paints its places'
Ui-ExpectLog 'Notice hover highlighted: message band: ' 'the message band paints its places'
Ui-ExpectLog 'Notice hover highlighted: panel: ' 'the command panel''s message paints its places'
Ui-ExpectLog 'TestDriver: done' 'the notice places are painted on hover'
Ui-StopApp
Write-Host 'PASS: notice places are painted on hover'
