# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-title-block-sheets.mitcad-ui tools/ui-scripts/drawing-title-block-sheets.json
# Delete Sheet while the title block is edited (mitcad#1841) in the build
# VM, as tools/ui-drawing-title-block-sheets-test.sh on Linux: the shared
# steps tools/ui-scripts/drawing-title-block-sheets.mitcad-ui on the
# design that mitcad-cli makes
# (tools/ui-scripts/drawing-title-block-sheets.json: a drawing of three ISO
# sheets): the third sheet's block edited and the first sheet deleted,
# Done writes the block onto the sheet edited and no other; the sheet
# edited deleted ends the editor.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-title-block-sheets-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-title-block-sheets'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheets.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-title-block-sheets.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-title-block-sheets.mitcad-ui'
Ui-StartApp 'drawing-title-block-sheets' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'title block sheets driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "title block sheets driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Title block editor: on, sheet 3: ' 'the editor on the third sheet'
Ui-ExpectLog 'Title block editor: cancelled' 'the sheet edited deleted ended the editor'
Ui-ExpectLog 'TestDriver: done' 'Done wrote the block onto the sheet edited after another sheet was deleted'
Ui-StopApp
Write-Host 'PASS: drawing title block with sheets deleted'
