# SPDX-License-Identifier: MIT
# check-all sources: app/browser core/model/src/document/acknowledged_warnings.rs tools/cli
# Acknowledged warnings (mitcad#1935) in the build VM, as
# tools/ui-acknowledge-warning-test.sh on Linux: the shared steps
# tools/ui-scripts/acknowledge-warning.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/timeline-auto-fix.json): an extrusion
# whose region a sketch change split warns; Acknowledge Warning takes it
# out of the warnings badge and the yellow cell, Show Warning Again brings
# both back, and the badge's menu acknowledges it too.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-acknowledge-warning-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-acknowledge-warning'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'warned.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/timeline-auto-fix.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/acknowledge-warning.mitcad-ui'
Ui-StartApp 'acknowledge-warning' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'acknowledged warning driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "acknowledged warning driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Acknowledged warning of Extrude1' 'Acknowledge Warning in the timeline menu'
Ui-ExpectLog 'TestDriver: done' 'an acknowledged warning leaves the badge and the yellow cell, and comes back'
Ui-StopApp
Write-Host 'PASS: acknowledged warnings'
