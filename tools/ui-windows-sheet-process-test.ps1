# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/commands/SheetProcessCommands.cpp core/model/src/sheet/process.rs app/framework/TestDriver.cpp tools/cli tools/ui-scripts/sheet-process.mitcad-ui tools/ui-scripts/sheet-process.json
# Sheet process profiles (mitcad#1939) in the build VM, as tools/ui-sheet-process-test.sh on Linux: the
# shared steps tools/ui-scripts/sheet-process.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sheet-process.json). Board rules with a crease table made in the Sheet Rules panel, Corner Relief greyed for them, a board sheet, the rules edited loading their profile.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-process-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-process'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-process.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-process.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-process.mitcad-ui'
Ui-StartApp 'sheet-process' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sheet-process driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet-process driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sheet rules: thickness .*, board, crease_table' 'board rules with a crease table were added'
Ui-ExpectLog 'Editing F2 with Sheet Rules' 'the rules were edited'
Ui-ExpectLog 'TestDriver: done' 'Sheet process profiles (mitcad#1939): every step of the script'
Ui-StopApp
Write-Host 'PASS: Sheet process profiles (mitcad#1939)'
