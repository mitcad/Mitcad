# SPDX-License-Identifier: MIT
# check-all sources: app/report app/framework/TestDriver.cpp app/platform/MacChrome.hpp tools/cli
# check-all sources: tools/ui-scripts/feedback-region.mitcad-ui tools/ui-scripts/feedback-region.json
# Send Feedback on a Region (mitcad#2616) on Windows: the shared input
# script tools/ui-scripts/feedback-region.mitcad-ui as
# tools/ui-feedback-region-test.sh runs it on Linux. F9, a drag over the
# view and a browser row on the overlay, the form with Bug and the region's
# screenshot, private; Esc and a click without a drag cancel; F9 held
# released after the drag takes the region.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-feedback-region'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'region.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/feedback-region.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'feedback region fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/feedback-region.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'feedback-region' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'feedback region driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "feedback region driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Feedback form for a region: kind bug, screenshot on' 'the form for a region'
Ui-ExpectLog 'Feedback region: canceled (Esc)' 'Esc cancels'
Ui-ExpectLog 'TestDriver: done' 'Send Feedback on a Region'
Ui-StopApp
Write-Host 'PASS: feedback region'
