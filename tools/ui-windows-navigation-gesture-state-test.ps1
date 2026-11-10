# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverNavigation.cpp tools/ui-scripts/navigation-gesture-state.mitcad-ui
# A press on the 3D view stays what it began as until its button is up
# (mitcad#1571, #1572, #1573) in the build VM, as
# tools/ui-navigation-gesture-state-test.sh on Linux: the shared steps
# tools/ui-scripts/navigation-gesture-state.mitcad-ui on the demo block, in
# the Floating and the Docked layout: keys pressed or released during a
# press keep its gesture and its click, a trackpad scroll while a button is
# held leaves the press a click, a scroll's end lets its button up at once,
# and the window deactivated during a drag ends it, as does F4 released
# elsewhere.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-navigation-gesture-state-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-navigation-gesture-state'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/navigation-gesture-state.mitcad-ui'
foreach ($chrome in @('floating', 'docked')) {
  Write-Host "=== $chrome"
  Ui-StartApp "navigation-gesture-state-$chrome" @('--ui=qt', '--no-mcp', '--demo', "--chrome=$chrome")
  # The handle read while the app runs: without it ExitCode stays empty
  # after the exit (Start-Process -PassThru).
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${chrome}: navigation gesture state driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${chrome}: navigation gesture state driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Navigation gesture state: keys changed during a press kept its gesture and its click' `
    "${chrome}: keys changed during a press"
  Ui-ExpectLog 'Navigation gesture state: a scroll with a button held' "${chrome}: scrolls during a press"
  Ui-ExpectLog 'Navigation gesture state: a deactivated window ended F4, Option and scroll drags' `
    "${chrome}: the window deactivated during a drag"
  Ui-ExpectLog 'TestDriver: done' "${chrome}: Ctrl released before a moved click, and F4 released elsewhere"
  Ui-StopApp
}
Write-Host 'PASS: navigation gesture state'
