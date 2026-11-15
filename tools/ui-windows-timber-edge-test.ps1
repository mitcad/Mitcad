# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/commands/DerivedPartCommands.cpp app/framework/TestDriver.cpp app/library tools/cli
# check-all sources: core/model/src/features/edge_frame.rs core/model-build core/model-api/src tools/ui-windows-timber-lib.ps1
# Timber's Along an edge (mitcad#1814) in the build VM, as
# tools/ui-timber-edge-test.sh on Linux: the shared steps
# tools/ui-scripts/timber-edge.mitcad-ui on a plate that mitcad-cli makes
# (tools/ui-scripts/timber-edge.json), with the synthetic timber library of
# tools/ui-windows-timber-lib.ps1 (its dimensioned 48x98 the member): a
# member along the plate's long edge, flush and then moved by Inset, Start,
# End and Lift; the plate made shorter, the member follows with its offsets
# kept; a round edge is refused in words.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-edge-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-edge'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber along an edge test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

$design = Join-Path $script:work 'timber-edge.mitcad'
Run-Cli 'run timber-edge.json' @('run', (Join-Path $PSScriptRoot 'ui-scripts/timber-edge.json'), '--save', $design)
$env:MITCAD_TEST_WORK = $script:work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/timber-edge.mitcad-ui'
Ui-StartApp 'timber-edge' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Along an edge driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Along an edge driver failed (exit code $exitCode)"
}
Ui-ExpectLog '0.00, 49.00, 33.00; inset 0 mm, start 0 mm, end 0 mm, lift 0 mm;' `
  'the member along the long edge, 1800 long, flush on the top face'
Ui-ExpectLog 'length along an edge, 1730.00 mm; end A at 50.00, 59.00, 38.00' `
  'Inset 10, Start 50, End 20, Lift 5: placed where they say'
Ui-ExpectLog 'TestDriver: done' 'the plate 1500 long: the member 1430 long, its offsets kept; a round edge refused'
Ui-StopApp
Write-Host "PASS: $TimberTest"
