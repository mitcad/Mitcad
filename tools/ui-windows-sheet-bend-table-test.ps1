# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSheet.cpp app/resources/layout/default.json core/model/src/sheet/bend_table.rs
# check-all sources: core/model/src/document/flat_patterns.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/sheet-bend-table.mitcad-ui tools/ui-scripts/sheet-flange-remembered.json
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-bend-table'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
Remove-Item (Join-Path $work 'bends.csv'), (Join-Path $work 'bends.json') -ErrorAction SilentlyContinue
$design = Join-Path $work 'bracket.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-remembered.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-bend-table.mitcad-ui'
Ui-StartApp 'sheet-bend-table' @('--ui=qt', '--no-native-dialogs', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'Export Bend Table timed out' }
if ($script:UiProcess.ExitCode -ne 0) { Ui-Fail "Export Bend Table failed (exit code $($script:UiProcess.ExitCode))" }
Ui-ExpectLog 'TestDriver: done' 'the bend table exports'
$row = Get-Content (Join-Path $work 'bends.csv') | Select-Object -Skip 1
if ($row -notmatch '^"[^"]+","SheetRules1",2,0[.]3,"F4:0",90,"(up|down)",3,') {
  Ui-Fail 'the CSV bend row has the expected sheet and bend values'
}
Write-Host $row
$table = Get-Content -Raw (Join-Path $work 'bends.json') | ConvertFrom-Json
if ($table.rows[0].angle_deg -ne 90) { Ui-Fail 'the JSON bend row has the expected angle' }
Ui-StopApp
Write-Host 'PASS: Qt Export Bend Table'
