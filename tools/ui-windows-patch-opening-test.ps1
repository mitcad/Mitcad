# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/framework/OpeningEdges.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp tools/cli
# Patch picks a surface body's free edge as its opening and closes it
# (mitcad#1927, GitHub mitcad/Mitcad#379) in the build VM, as
# tools/ui-patch-opening-test.sh on Linux: the shared steps
# tools/ui-scripts/patch-opening.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/patch-opening.json: a box whose top face was
# unstitched and deleted): the opening's free edges are offered as edges,
# with Whole Opening on a click on one takes the whole opening; with it off
# (mitcad#1955) the four edges picked one by one close the loop, and OK
# with Stitch makes the box a closed solid again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-patch-opening-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-patch-opening'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'patch-opening.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/patch-opening.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/patch-opening.mitcad-ui'
Ui-StartApp 'patch-opening' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Patch opening driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Patch opening driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Patch Profiles or Edges: 1 edge [edge {"faces":' 'with Whole Opening on a click takes the whole opening'
Ui-ExpectLog 'Patch: Whole Opening = off' 'Whole Opening turned off'
Ui-ExpectLog 'Added patch of 4 items, stitched' 'four single free edges patched and stitched on'
Ui-ExpectLog 'TestDriver: done' 'the box is a closed solid again'
Ui-StopApp
Write-Host 'PASS: Patch picks an opening by its free edge and closes it'
