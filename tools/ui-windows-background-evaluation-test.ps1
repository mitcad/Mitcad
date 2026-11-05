# SPDX-License-Identifier: MIT
# check-all sources: app/framework/DocumentSession.cpp app/framework/TestDriver.cpp core/ffi/src/background.rs tools/cli
# The rolled-back features evaluated in the background (mitcad#1267) in
# the build VM, as tools/ui-background-evaluation-test.sh on Linux: the
# shared steps tools/ui-scripts/background-evaluation.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/background-evaluation.json):
# a join added before the marker, its rolled-back fillets evaluated in the
# background and adopted, the marker dragged to the end evaluating nothing;
# an edit that opens while a background evaluation runs cancels it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-background-evaluation-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-background-evaluation'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'background.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/background-evaluation.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/background-evaluation.mitcad-ui'
# Each background evaluation 4 s longer, so that a command starts while one
# runs (Diagnostics.hpp).
$env:MITCAD_TEST_RECOMPUTE_DELAY_MS = 'background evaluation=4000'
Ui-StartApp 'background-evaluation' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'background evaluation driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "background evaluation driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the marker move after the background evaluation evaluates nothing; a command cancels it'
Ui-StopApp
Write-Host 'PASS: background evaluation of the rolled-back features'
