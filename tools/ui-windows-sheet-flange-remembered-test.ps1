# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/RememberedValues.cpp app/framework/TestDriver.cpp tools/cli
# Flange's remembered choices (mitcad#1868, GitHub mitcad/Mitcad#348) in
# the build VM, as tools/ui-sheet-flange-remembered-test.sh on Linux: the
# shared steps tools/ui-scripts/sheet-flange-remembered.mitcad-ui on the
# design that mitcad-cli makes
# (tools/ui-scripts/sheet-flange-remembered.json): a Flange with Measured
# Inner Length and Position Outside, OK; Flange again shows both; the
# earlier flange edited loads its own Tangent Length and Bend Line; an
# edit's OK keeps nothing. The test library starts panels from their
# defaults (MITCAD_REMEMBER_PANEL_VALUES=0); this test turns the remembered
# values on, in the settings of its own that Ui-Init makes.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-flange-remembered-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-flange-remembered'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
# Remembered values on for this test (the library leaves a set value).
$env:MITCAD_REMEMBER_PANEL_VALUES = '1'
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-flange-remembered.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-remembered.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-remembered.mitcad-ui'
Ui-StartApp 'sheet-flange-remembered' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sheet flange remembered driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet flange remembered driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added Flange as F5' 'the flange with Inner Length at Outside was added'
Ui-ExpectLog 'Panel Flange position: remembered outside' 'Flange again started with the last OK''s Position'
Ui-ExpectLog 'Edited F5' 'the new flange was edited'
Ui-ExpectLog 'TestDriver: done' 'a new Flange starts with the last OK''s Measured and Position, an edit loads its own'
Ui-StopApp
Remove-Item Env:MITCAD_REMEMBER_PANEL_VALUES -ErrorAction SilentlyContinue
Write-Host 'PASS: a new Flange starts with the last OK''s Measured and Position'
