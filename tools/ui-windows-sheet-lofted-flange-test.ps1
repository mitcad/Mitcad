# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# Lofted Flange (mitcad#2512) in the build VM, as
# tools/ui-sheet-lofted-flange-test.sh on Linux: the shared steps
# tools/ui-scripts/sheet-lofted-flange.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sheet-lofted-flange.json): a rectangle
# and a circle 70 mm above it picked as the profiles, OK, the press brake
# transition one body of the volume cli.sheet_lofted_flange gives; the
# lofted flange edited loads its profiles.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-lofted-flange-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-lofted-flange'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-lofted-flange.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-lofted-flange.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-lofted-flange.mitcad-ui'
Ui-StartApp 'sheet-lofted-flange' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sheet lofted flange driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet lofted flange driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Lofted Flange: ok' 'the rectangle and the circle previewed as a lofted flange'
Ui-ExpectLog 'Added Lofted Flange as F5' 'the lofted flange was added'
Ui-ExpectLog 'Editing F5 with Lofted Flange' 'the lofted flange was edited'
Ui-ExpectLog 'TestDriver: done' 'a press brake transition made through the panel'
Ui-StopApp
Write-Host 'PASS: a press brake transition made through the Lofted Flange panel'
