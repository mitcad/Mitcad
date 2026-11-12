# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPanel.hpp app/browser/BrowserController.cpp
# check-all sources: app/browser/BrowserPreferences.cpp app/browser/BrowserPreferences.hpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/browser-used-sketches.mitcad-ui tools/ui-scripts/browser-used-sketches.json
# Show used sketches under their features (mitcad#1699) in the build VM, as
# tools/ui-browser-used-sketches-test.sh on Linux: the shared steps
# tools/ui-scripts/browser-used-sketches.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/browser-used-sketches.json): off, the
# Browser as before; on, a Features folder at once with Sketch1 under
# Extrude1 and Extrude2 (the shared mark on both), Sketch3 under Extrude3
# (no mark) and Sketch2 left in Sketches; Edit Sketch from Sketch1's row
# under Extrude2 opens Sketch1; off again, the rows as before.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-used-sketches-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-used-sketches'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'used.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-used-sketches.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/browser-used-sketches.mitcad-ui'
Ui-StartApp 'browser-used-sketches' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'browser used sketches driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "browser used sketches driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Browser used sketches under their features: on' 'the preference turned on in Preferences, applied at once'
Ui-ExpectLog 'Browser shared Root/Features/Extrude1/Sketch1: used by 2 features' 'Sketch1 under Extrude1, marked shared'
Ui-ExpectLog 'Browser shared Root/Features/Extrude2/Sketch1: used by 2 features' 'and under Extrude2, marked shared'
$marked = @(Ui-LogLines | Where-Object { $_.StartsWith('Browser shared Root/Features/Extrude3/') }).Count
if ($marked -ne 0) { Ui-Fail 'Sketch3, used by Extrude3 alone, is marked shared' }
Write-Host 'ok   Sketch3 under Extrude3, unmarked'
Ui-ExpectLog 'Editing sketch F1 (Sketch1)' 'Edit Sketch from the row under Extrude2 opens Sketch1'
Ui-ExpectLog 'Browser used sketches under their features: off' 'turned off again'
Ui-ExpectLog 'TestDriver: done' 'the Browser shows used sketches under their features and goes back'
Ui-StopApp
Write-Host 'PASS: browser used sketches'
