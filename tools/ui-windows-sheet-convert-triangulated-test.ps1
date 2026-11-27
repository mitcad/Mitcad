# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/sheet tools/cli
# Convert to Sheet Metal by triangulation (mitcad#2442) in the build VM, as
# tools/ui-sheet-convert-triangulated-test.sh on Linux: the shared steps
# tools/ui-scripts/sheet-convert-triangulated.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sheet-convert-triangulated.json): a
# shelled rectangle-to-round transition's four outer faces picked with
# Development Approximate, OK, the body kept and its Flat Pattern
# approximate; edited with fewer triangles, its stated deviation grows.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-convert-triangulated-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-convert-triangulated'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-convert-triangulated.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-convert-triangulated.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-convert-triangulated.mitcad-ui'
Ui-StartApp 'sheet-convert-triangulated' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet convert triangulated driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet convert triangulated driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Convert to Sheet Metal: ok' 'the transition with its four faces previewed as a sheet'
Ui-ExpectLog 'Added Convert to Sheet Metal as F7' 'the conversion was added'
Ui-ExpectLog 'Editing F7 with Convert to Sheet Metal' 'the conversion was edited'
Ui-ExpectLog 'TestDriver: done' 'a transition laid flat by triangulation through the panel'
Ui-StopApp
Write-Host 'PASS: a transition laid flat by triangulation through the panel'
