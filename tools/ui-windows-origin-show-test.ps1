# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowVisibility.cpp app/MainWindow.cpp
# check-all sources: app/OcctViewer.cpp app/framework/TestDriver.cpp tools/ui-scripts/origin-show.mitcad-ui
# The design's origin shown and hidden both ways (mitcad#1642, GitHub
# mitcad/Mitcad#240) in the build VM, as tools/ui-origin-show-test.sh on
# Linux: the shared steps tools/ui-scripts/origin-show.mitcad-ui on a new
# design. The Origin folder's eye and View > Show > Origins each show the
# origin's planes, axes and point (the other control turned on in the same
# undo step) and hide them again, read from the view itself ("View origin
# drawn: ...", expect-view-origin: the planes' colour in the view's frame
# too).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-origin-show-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-origin-show'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/origin-show.mitcad-ui'
Ui-StartApp 'origin-show' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'origin show driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "origin show driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: view origin: planes xy, xz, yz; axes x, y, z; point origin; ' "the origin's planes, axes and point drawn"
Ui-ExpectLog 'TestDriver: view origin: planes none; axes none; point none; ' 'the origin hidden again'
Ui-ExpectLog 'Undo: Show Origin' 'the eye showed the origin in one undo step'
Ui-ExpectLog 'Undo: Show All Origins' 'View > Show > Origins showed it in one undo step'
Ui-ExpectLog 'TestDriver: done' 'the origin shows and hides with the eye and with View > Show > Origins'
Ui-StopApp
Write-Host 'PASS: the origin shown both ways'
