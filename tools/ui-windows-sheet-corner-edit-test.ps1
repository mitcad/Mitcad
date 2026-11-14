# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp core/model/src/sheet core/model/src/document/flat_patterns.rs tools/cli
# A saved Corner Round beside a bend edited from the timeline (mitcad#1766)
# in the build VM, as tools/ui-sheet-corner-edit-test.sh on Linux: the
# shared steps tools/ui-scripts/sheet-corner-edit.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/sheet-corner-edit.json): once the
# timeline is rolled back to the feature, its corner is found on the edge
# between the flange's wall and the bend's side, which the panel lists and
# the view highlights.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-corner-edit-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-corner-edit'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-corner-edit.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-corner-edit.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-corner-edit.mitcad-ui'
Ui-StartApp 'sheet-corner-edit' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet corner edit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet corner edit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Corner Round corners where the feature finds them: edge E{F6:sheet_bend_side0' 'the corner was found on the edge beside the bend'
Ui-ExpectLog 'TestDriver: done' 'the edited Corner Round lists and highlights its corner beside the bend'
Ui-StopApp
Write-Host 'PASS: the edited Corner Round lists and highlights its corner beside the bend'
