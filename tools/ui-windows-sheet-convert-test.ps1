# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# Convert to Sheet Metal (mitcad#1949, GitHub mitcad/Mitcad#377) in the
# build VM, as tools/ui-sheet-convert-test.sh on Linux: the shared steps
# tools/ui-scripts/sheet-convert.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sheet-convert.json): a solid box's bottom face
# and its four bottom edges picked, OK, the tray one body of the volume
# cli.sheet_convert_tray gives, its corners ripped; the conversion edited
# loads its picks.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-convert-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-convert'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-convert.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-convert.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-convert.mitcad-ui'
Ui-StartApp 'sheet-convert' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sheet convert driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet convert driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Convert to Sheet Metal: ok' 'the box with its base and four bend edges previewed as a sheet'
Ui-ExpectLog 'Added Convert to Sheet Metal as F3' 'the conversion was added'
Ui-ExpectLog 'Editing F3 with Convert to Sheet Metal' 'the conversion was edited'
Ui-ExpectLog 'TestDriver: done' 'a solid tray converted through the panel, its corners ripped'
Ui-StopApp
Write-Host 'PASS: a solid tray converted to sheet metal through the panel'
