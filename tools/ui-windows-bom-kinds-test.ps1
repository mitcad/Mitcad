# SPDX-License-Identifier: MIT
# check-all sources: app/build/BuildLists.cpp app/build/ListTables.cpp app/framework/TestDriver.cpp tools/cli
# The Bill of Materials' Kind and the Sheet Metal list (mitcad#1980) with
# the BOM Structure (mitcad#1985) in the build VM, as
# tools/ui-bom-kinds-test.sh on Linux: the shared steps
# tools/ui-scripts/bom-kinds.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/bom-kinds.json: two sheet-metal components, one placed
# twice, and a plain solid): Kind and BOM Structure as property columns
# and as the window's filters, the Sheet Metal tab with
# its developed blanks, its CSV and PDF, and its parts' Flat Patterns added
# to a new cut sheet.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-bom-kinds-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-bom-kinds'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
# The file of an earlier run: the save dialog would ask to replace it.
$pdf = Join-Path $work 'sheet-metal.pdf'
Remove-Item -Force $pdf -ErrorAction SilentlyContinue
$design = Join-Path $work 'bom-kinds.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/bom-kinds.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/bom-kinds.mitcad-ui'
Ui-StartApp 'bom-kinds' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Bill of Materials kinds driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Bill of Materials kinds driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Build parts list row: Bracket: Sheet metal | Normal' 'Kind and BOM Structure as property columns'
Ui-ExpectLog 'Build parts list filter: kind Sheet metal, structure all' 'the Kind filter'
Ui-ExpectLog 'Build parts list filter: kind all, structure Purchased' 'the BOM Structure filter'
Ui-ExpectLog 'Sheet metal list: 2 row(s)' 'the Sheet Metal tab'
Ui-ExpectLog 'Sheet metal parts added to CutSheet1: 3 part(s)' 'the Flat Patterns on a new cut sheet'
Ui-ExpectLog 'TestDriver: done' 'every step of the Bill of Materials kinds passed'
Ui-StopApp
if (-not (Test-Path $pdf)) { Ui-Fail 'sheet-metal.pdf was not written' }
$head = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($pdf), 0, 5)
if ($head -ne '%PDF-') { Ui-Fail 'sheet-metal.pdf is no PDF' }
Write-Host 'ok   sheet-metal.pdf: a PDF'
Write-Host 'PASS: Bill of Materials kinds'
