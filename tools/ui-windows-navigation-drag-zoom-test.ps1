# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverNavigation.cpp tools/ui-scripts/navigation-drag-zoom.mitcad-ui
# A drag zoom follows the pointer's vertical motion (mitcad#1867, GitHub
# mitcad/Mitcad#330) in the build VM, as tools/ui-navigation-drag-zoom-test.sh
# on Linux: the shared steps tools/ui-scripts/navigation-drag-zoom.mitcad-ui
# on the demo block, in the Floating and the Docked layout: with the
# middle-orbit scheme, Shift + middle dragged straight up comes closer,
# straight down goes away, sideways alone keeps the zoom, and up with a
# sideways wobble comes closer at every step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-navigation-drag-zoom-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-navigation-drag-zoom'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/navigation-drag-zoom.mitcad-ui'
foreach ($chrome in @('floating', 'docked')) {
  Write-Host "=== $chrome"
  Ui-StartApp "navigation-drag-zoom-$chrome" @('--ui=qt', '--no-mcp', '--demo', "--chrome=$chrome")
  # The handle read while the app runs: without it ExitCode stays empty
  # after the exit (Start-Process -PassThru).
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${chrome}: drag zoom driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${chrome}: drag zoom driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Drag zoom: straight up came closer, straight down went away, sideways kept the zoom, an upward drag with a sideways wobble came closer at every step (Shift + middle)' `
    "${chrome}: Shift + middle zooms by the vertical motion"
  Ui-ExpectLog 'TestDriver: done' "${chrome}: drag zoom completed"
  Ui-StopApp
}
Write-Host 'PASS: navigation drag zoom'
