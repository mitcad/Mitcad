# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model-api/src/sheet_process.rs core/model-api/src/sheet_process core/model/src/sheet app/framework/TestDriver.cpp tools/cli tools/ui-scripts/board-box.mitcad-ui tools/ui-scripts/board-box.json
# Standard boxes of board (mitcad#1939) in the build VM, as tools/ui-board-box-test.sh on Linux: the
# shared steps tools/ui-scripts/board-box.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/board-box.json). A FEFCO 0201 box of corrugated board made by Standard Box, folded with its glue flap touching the first panel, Finger Joint greyed for the board, the box taken back by one undo; then a FEFCO 0203 box and a FEFCO 0427 folder chosen from the Style list and built (mitcad#2226); then a FEFCO 0421 folder with its Tuck and Tabs set, kept in its rules, its tuck changed in Edit Standard Box (mitcad#2269), then its number of tabs, the blank made again and the box following, and another style refused by the model in words in the panel's own message area (mitcad#2285).
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
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'board-box driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "board-box driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Standard box fefco_0201, ' 'the standard box was made'
Ui-ExpectLog 'Standard box fefco_0203, ' 'a FEFCO 0203 box was made from the Style list (mitcad#2226)'
Ui-ExpectLog 'Standard box fefco_0427, ' 'a FEFCO 0427 folder was made from the Style list (mitcad#2226)'
Ui-ExpectLog ', tuck 25 mm, tabs 3' 'a FEFCO 0421 folder was made with Tuck 25 mm and Tabs 3 (mitcad#2269)'
Ui-ExpectLog ', tuck 30 mm, tabs 3' 'Edit Standard Box changed the folder tuck to 30 mm (mitcad#2269)'
Ui-ExpectLog ', tuck 30 mm, tabs 4' 'Edit Standard Box made the folder blank again with four tabs, the box following (mitcad#2285)'
Ui-ExpectLog 'Panel Standard Box error: another style changes the shape of the box' 'another style was refused in words in the panel (mitcad#2285)'
Ui-ExpectLog 'Standard Box: OK refused by the model: ' 'the model refusal kept the panel open (mitcad#2285)'
Ui-ExpectLog 'TestDriver: done' 'Standard boxes of board (mitcad#1939): every step of the script'
Ui-StopApp
Write-Host 'PASS: Standard boxes of board (mitcad#1939)'
