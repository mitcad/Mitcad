# SPDX-License-Identifier: MIT
# check-all sources: app/browser core/model/src/document/auto_fix.rs tools/cli
# Auto Fix in the timeline's context menu (mitcad#1223) in the build VM,
# as tools/ui-timeline-auto-fix-test.sh on Linux: the shared steps
# tools/ui-scripts/timeline-auto-fix.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/timeline-auto-fix.json): an extrusion
# whose region a sketch change split warns; Auto Fix in its timeline menu
# fixes it as one undo step, and the warning comes back after Undo. The
# tooltips name Auto Fix, and the warnings badge offers it (mitcad#1677).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timeline-auto-fix-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timeline-auto-fix'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'auto-fix.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/timeline-auto-fix.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/timeline-auto-fix.mitcad-ui'
Ui-StartApp 'timeline-auto-fix' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'timeline Auto Fix driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "timeline Auto Fix driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Auto Fix from the timeline menu: the warning goes, one undo step'
Ui-StopApp
Write-Host 'PASS: Auto Fix in the timeline menu'
