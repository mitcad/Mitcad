# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/resources/layout/default.json
# check-all sources: core/model/src/features/sheet_perforate.rs core/model/src/sheet/folded/perforate.rs
# check-all sources: tools/ui-scripts/sheet-perforate.mitcad-ui tools/ui-scripts/sheet-perforate.json
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-perforate'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'perforate.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-perforate.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-perforate.mitcad-ui'
Ui-StartApp 'sheet-perforate' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Perforate timed out' }
if ($script:UiProcess.ExitCode -ne 0) { Ui-Fail "Perforate failed (exit code $($script:UiProcess.ExitCode))" }
Ui-ExpectLog 'The perforation cuts 11 holes.' 'the outline limits the field'
Ui-ExpectLog 'TestDriver: done' 'all Perforate steps completed'
Ui-StopApp
Write-Host 'PASS: Qt Perforate'
