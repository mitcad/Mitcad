# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/framework/CommandSession.cpp geometry/src/surfaces.cpp
# check-all sources: tools/ui-scripts/surface-patch-curves.mitcad-ui app/framework/TestDriver.cpp
# Patch closes part of an opening with two single free edges and a sketch
# line (mitcad#1972, GitHub mitcad/Mitcad#407), in the build VM: the shared
# steps tools/ui-scripts/surface-patch-curves.mitcad-ui (also run by
# tools/ui-surface-patch-curves-test.sh and
# tools/ui-macos-surface-patch-curves-test.sh) on the reported design of
# the private test files (MITCAD_MITCAD_REPORTED,
# github-407/surfacing_bug.zip; never committed), unpacked into the test's
# folder: with Whole Opening off Patch takes the wall's top and slanted free
# edges and a sketch line across the wall, and OK adds the patch.
# Skipped (77) without the design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-surface-patch-curves-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-surface-patch-curves'),
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
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/surface-patch-curves.mitcad-ui'
Ui-StartApp 'surface-patch-curves' @('--ui=qt', '--no-mcp', '--open', $design.FullName)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(900000)) { Ui-Fail 'surface patch curves driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "surface patch curves driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Patch: Whole Opening = off' 'Whole Opening is turned off'
Ui-ExpectLog 'Patch Profiles or Edges: 2 edges, 1 sketch curve' 'two single free edges and the sketch line are taken'
Ui-ExpectLog 'Preview Patch: ok' 'the patch of the loop previews'
Ui-ExpectLog 'Added patch of 3 items' 'OK adds the patch'
Ui-ExpectLog 'TestDriver: done' 'every step passed'
Ui-StopApp
Write-Host 'PASS: patch of free edges and a sketch line'
