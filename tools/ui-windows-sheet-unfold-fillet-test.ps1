# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp core/model/src/sheet core/model/src/sheet.rs core/model/src/features tools/cli
# An Unfold refused by a fillet on a surface edge (mitcad#1942) in the
# build VM, as tools/ui-sheet-unfold-fillet-test.sh on Linux: the shared
# steps tools/ui-scripts/sheet-unfold-fillet.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sheet-unfold-fillet.json): the edited
# Unfold's panel says why in words, the fillet by its name, no ids.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-unfold-fillet-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-unfold-fillet'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-unfold-fillet.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-unfold-fillet.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-unfold-fillet.mitcad-ui'
Ui-StartApp 'sheet-unfold-fillet' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet unfold fillet driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet unfold fillet driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Panel Unfold error: sheet Unfold needs the sheet' 'the Unfold panel says why it fails'
Ui-ExpectLog 'TestDriver: done' 'the Unfold panel names the fillet in words, without ids'
Ui-StopApp
Write-Host 'PASS: the Unfold panel names the fillet in words, without ids'
