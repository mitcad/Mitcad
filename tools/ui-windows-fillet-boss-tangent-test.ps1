# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: third_party/vcpkg-ports/opencascade/portfile.cmake geometry/src/dressup.cpp geometry/src/guard.cpp
# The part of GitHub mitcad/Mitcad#173 (mitcad#1422) built through the real
# UI and its fillet in the build VM, as tools/ui-fillet-boss-tangent-test.sh
# on Linux, with the same input scripts:
#   1. tools/ui-scripts/fillet-boss-tangent.mitcad-ui on the empty sketch
#      that mitcad-cli makes (tools/ui-scripts/fillet-boss-tangent.json): a
#      Center to Center Slot extruded 15, a circle on its end extruded 30
#      and joined, then Fillet on the plate's top edge running into the boss
#      with Tangent Chain: 30 mm fails with its error in the panel, 3 mm
#      makes a valid rounding of the exact volume; saved, and recomputed by
#      mitcad-cli with Fillet1 ok.
#   2. tools/ui-scripts/fillet-boss-tangent-crash.mitcad-ui on that design,
#      with MITCAD_TEST_OCCT_CRASH=fillet (an access violation inside OCCT):
#      the fillet's crash, as the design opens and in a new Fillet's
#      preview, is an error (in the timeline, in the panel) and the
#      application goes on.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fillet-boss-tangent-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fillet-boss-tangent'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fillet-boss-tangent.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/fillet-boss-tangent.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

function Wait-Driver([string]$What) {
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "${What}: driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${What}: the driver failed or the application ended (exit code $exitCode)"
  }
}

Write-Host '--- 1. The part through the UI, and its fillet'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-boss-tangent.mitcad-ui'
Ui-StartApp 'fillet-boss-tangent' @('--ui=qt', '--no-mcp', '--open', $design)
Wait-Driver 'the part and its fillet'
Ui-ExpectLog 'Added slot, width 50 mm' 'the slot drawn'
Ui-ExpectLog 'Added circle, diameter 50 mm at (85, 0)' "the circle on the slot's end"
Ui-ExpectLog 'Added extrude (Join), distance 30' 'the boss joined'
Ui-ExpectLog 'Panel Fillet error: ' 'the 30 mm fillet failed with its error in the panel'
Ui-ExpectLog 'Added Fillet' 'the 3 mm fillet made'
Ui-ExpectLog 'TestDriver: done' 'the part built through the UI and its edge rounded at 3 mm with Tangent Chain'
Ui-StopApp
$info = & $Cli info $design 2>&1 | Out-String
if ($LASTEXITCODE -ne 0 -or -not $info.Contains('Fillet1 (fillet): ok')) {
  Write-Host $info
  Ui-Fail 'Fillet1 does not recompute ok'
}
Write-Host 'ok   Fillet1 recomputes ok'

Write-Host '--- 2. A fillet that crashes inside OCCT'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-boss-tangent-crash.mitcad-ui'
$env:MITCAD_TEST_OCCT_CRASH = 'fillet'
try {
  Ui-StartApp 'fillet-boss-tangent-crash' @('--ui=qt', '--no-mcp', '--open', $design)
  Wait-Driver 'the crashing fillet'
} finally {
  Remove-Item Env:MITCAD_TEST_OCCT_CRASH -ErrorAction SilentlyContinue
}
Ui-ExpectLog 'Preview Fillet: failed: ' "the crashing fillet's preview failed"
Ui-ExpectLog 'Panel Fillet error: ' 'its error in the panel'
Ui-ExpectLog 'Command Fillet cancelled' 'Escape cancelled the command, the application still running'
Ui-ExpectLog 'TestDriver: done' 'a fillet that crashes inside OCCT is an error, not the end of the application'
Ui-StopApp
Write-Host 'PASS: fillet of a plate edge running tangentially into a boss'
