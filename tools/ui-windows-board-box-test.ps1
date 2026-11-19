# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model-api/src/sheet_process.rs core/model/src/sheet app/framework/TestDriver.cpp tools/cli tools/ui-scripts/board-box.mitcad-ui tools/ui-scripts/board-box.json
# Standard boxes of board (mitcad#1939) in the build VM, as tools/ui-board-box-test.sh on Linux: the
# shared steps tools/ui-scripts/board-box.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/board-box.json). A FEFCO 0201 box of corrugated board made by Standard Box, folded with its glue flap touching the first panel, Finger Joint greyed for the board, the box taken back by one undo.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-board-box-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-board-box'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'board-box.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/board-box.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/board-box.mitcad-ui'
Ui-StartApp 'board-box' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'board-box driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "board-box driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Standard box fefco_0201, ' 'the standard box was made'
Ui-ExpectLog 'TestDriver: done' 'Standard boxes of board (mitcad#1939): every step of the script'
Ui-StopApp
Write-Host 'PASS: Standard boxes of board (mitcad#1939)'
