# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp core/model/src/sheet core/model/src/features/sheet_folded.rs tools/cli
# check-all sources: app/framework/CommandPanel.cpp app/framework/Command.hpp
# A Flange on several edges with mitred corners (mitcad#1872, GitHub
# mitcad/Mitcad#337) in the build VM, as tools/ui-sheet-flange-edges-test.sh
# on Linux: the shared steps tools/ui-scripts/sheet-flange-edges.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/sheet-flange-edges.json):
# three side faces picked into one Flange show Miter, Own Corner Gap and
# Corners and leave the keyboard in Length (mitcad#2098), Corner Gap 1 mm and Position Inside, one corner's own Miter
# (mitcad#1877), Corner Gap 0 refused with its places painted on hover, OK
# adds one flange, and the flange edited again loads its three edges, its
# gap and its corner's Miter.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-flange-edges-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-flange-edges'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-flange-edges.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-edges.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-edges.mitcad-ui'
Ui-StartApp 'sheet-flange-edges' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet flange edges driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet flange edges driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Panel field length: 12' 'the keyboard in Length after the picks'
Ui-ExpectLog 'Added flange on 3 edges' 'the three edges made one flange'
Ui-ExpectLog 'Added Flange as F4' 'the flange was added'
Ui-ExpectLog 'Notice hover highlighted: panel: face F3:sheet_side' "the refusal's places are painted on hover"
Ui-ExpectLog 'TestDriver: done' 'the flange on three edges is made and edited from the Flange panel'
Ui-StopApp
Write-Host 'PASS: a Flange on three edges is made and edited from its panel'
