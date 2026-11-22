# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BoundaryFillCommands.cpp core/model/src/features/boundary_fill.rs geometry/src/boundary_fill.cpp tools/cli
# Boundary Fill (mitcad#2149) in the build VM: the shared steps
# tools/ui-scripts/boundary-fill.mitcad-ui (also run by tools/ui-boundary-fill-test.sh
# and tools/ui-macos-boundary-fill-test.sh) on the design mitcad-cli makes
# (tools/ui-scripts/boundary-fill.json): four surfaces and two planes filled
# as a new body, a clicked cell cut from a block, Undo, and the first fill
# edited with its cell.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-boundary-fill-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-boundary-fill'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'boundary-fill.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/boundary-fill.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/boundary-fill.mitcad-ui'
Ui-StartApp 'boundary-fill' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'boundary fill driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "boundary fill driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the closed region filled, a clicked cell cut, the fill edited with its cell'
Ui-StopApp
Write-Host 'PASS: Boundary Fill fills the closed region, cuts a clicked cell and edits with its cells'
