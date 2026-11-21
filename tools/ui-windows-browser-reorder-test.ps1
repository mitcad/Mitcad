# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/occurrence_order.rs tools/cli tools/ui-scripts/browser-reorder.mitcad-ui
# The order of components in the Browser (mitcad#2104) in the build VM, as
# tools/ui-browser-reorder-test.sh on Linux: the shared steps
# tools/ui-scripts/browser-reorder.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/browser-reorder.json): the third component dragged
# before the first, Move Down, Undo once, a drop onto another component's
# folder refused; the saved design opened again and checked with
# browser-reorder-check.json.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-reorder-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-reorder'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'browser-reorder.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-reorder.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$name = 'browser-reorder'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${name}: driver timed out" }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "${name}: driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
Ui-StopApp
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-reorder-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "${name}: the saved design does not pass browser-reorder-check.json" }
Write-Host 'PASS: components reordered by a drag and Move Down, undone, a wrong drop refused, the order kept with the design'
