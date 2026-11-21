# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/resources/layout/default.json core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# The Hem command (mitcad#2112) in the build VM, as
# tools/ui-sheet-hem-test.sh on Linux: the shared steps
# tools/ui-scripts/sheet-hem.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/sheet-hem.json): a Flat hem made on a side face, opened
# in Hem and changed to Open, Rolled and Double with their volumes, then
# unfolded and refolded.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-hem-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-hem'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-hem.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-hem.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-hem.mitcad-ui'
Ui-StartApp 'sheet-hem' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet hem driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet hem driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added Hem as F4' 'the flat hem was added'
Ui-ExpectLog 'Editing F4 with Hem' 'the hem opened in Hem'
Ui-ExpectLog 'TestDriver: done' 'a hem made, changed to Open, Rolled and Double, unfolded and refolded'
Ui-StopApp
Write-Host 'PASS: a hem made, changed to Open, Rolled and Double, unfolded and refolded'
