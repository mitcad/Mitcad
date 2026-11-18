# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/framework/CommandSession.cpp app/commands/SurfaceCommands.cpp geometry/src/surface_offsets.cpp
# check-all sources: tools/ui-scripts/surface-free-edges.mitcad-ui app/framework/TestDriver.cpp
# A surface body's free edges with no command, and Extend's Whole Opening
# (mitcad#1955, GitHub mitcad/Mitcad#407), in the build VM: the shared
# steps tools/ui-scripts/surface-free-edges.mitcad-ui (also run by
# tools/ui-surface-free-edges-test.sh and
# tools/ui-macos-surface-free-edges-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, github-407/surfacing_bug.zip;
# never committed), unpacked into the test's folder: with no command a free
# edge hovers and selects as an edge, Extend started then takes it as its
# whole opening, and with Whole Opening off the single edge extends.
# Skipped (77) without the design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-surface-free-edges-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-surface-free-edges'),
  [string]$QtBin = '')
$reportedRoot = $env:MITCAD_MITCAD_REPORTED
$zip = if ($reportedRoot) { Join-Path $reportedRoot 'github-407/surfacing_bug.zip' } else { '' }
if (-not $zip -or -not (Test-Path -LiteralPath $zip -PathType Leaf)) {
  Write-Host 'SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-407) is not here'
  exit 77
}
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$unpacked = Join-Path $work 'reported'
Expand-Archive -LiteralPath $zip -DestinationPath $unpacked -Force
$design = Get-ChildItem -LiteralPath $unpacked -Recurse -File -Filter '*.mitcad' | Sort-Object FullName | Select-Object -First 1
if (-not $design) { Ui-Fail 'no design in the reported zip' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/surface-free-edges.mitcad-ui'
Ui-StartApp 'surface-free-edges' @('--ui=qt', '--no-mcp', '--open', $design.FullName)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(900000)) { Ui-Fail 'surface free edges driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "surface free edges driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Idle hover: edge {"at":' 'with no command a free edge hovers as an edge'
Ui-ExpectLog 'Selected: 1 edge [edge {"at":' 'a click selects the free edge'
Ui-ExpectLog 'Extend Edges: 1 edge [edge {"faces":' 'Extend takes it as its whole opening'
Ui-ExpectLog 'Hover opening: 4 edge' 'the hover shows the whole opening'
Ui-ExpectLog 'Added extend of 1 edges' 'with Whole Opening off the single edge extends'
Ui-ExpectLog 'TestDriver: done' 'every step passed'
Ui-StopApp
Write-Host 'PASS: free edges picked with no command, Extend with and without Whole Opening'
