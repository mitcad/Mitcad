# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/Navigation.hpp app/view/NavigationPreferences.hpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/navigation-reverse-pan.mitcad-ui
# check-all sources: tools/ui-scripts/navigation-reverse-pan-restored.mitcad-ui
# Reverse vertical pan (mitcad#1873, GitHub mitcad/Mitcad#340) in the build
# VM, as tools/ui-navigation-reverse-pan-test.sh on Linux: the shared steps
# tools/ui-scripts/navigation-reverse-pan.mitcad-ui and
# navigation-reverse-pan-restored.mitcad-ui on the demo block. The three
# Reverse flags of Preferences > Navigation are off by default; with
# Reverse vertical pan on, a vertical two-finger scroll pans the view the
# other way and a horizontal one the same way; the setting is saved, read
# back by a new process (the Floating layout, then the Docked one) and
# turned off there again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-navigation-reverse-pan-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-navigation-reverse-pan'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
# The Preferences dialog keeps its changes with OK, its default button.
$env:MITCAD_TEST_PREFERENCES_DONE = 'Return'
foreach ($run in @(@('reverse-pan', 'floating'), @('reverse-pan-restored', 'docked'))) {
  $part = $run[0]
  $chrome = $run[1]
  Write-Host "=== $part ($chrome)"
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/navigation-$part.mitcad-ui"
  Ui-StartApp "navigation-$part" @('--ui=qt', '--no-mcp', '--demo', "--chrome=$chrome")
  # The handle read while the app runs: without it ExitCode stays empty
  # after the exit (Start-Process -PassThru).
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${part}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${part}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: the view panned the opposite way' `
    "${part}: a vertical two-finger scroll pans the other way when Reverse vertical pan changes"
  if ($part -eq 'reverse-pan') {
    Ui-ExpectLog 'TestDriver: the view panned the same way' `
      "${part}: Reverse vertical pan leaves the horizontal scroll as it was"
  }
  Ui-ExpectLog 'TestDriver: done' "${part}: the steps completed"
  Ui-StopApp
}
Write-Host 'PASS: navigation reverse pan'
