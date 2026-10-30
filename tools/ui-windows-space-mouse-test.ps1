# SPDX-License-Identifier: MIT
# check-all sources: app/view/SpaceMouseController.cpp app/view/SpaceMouseController.hpp
# check-all sources: app/view/SpaceMouseNavigation.hpp app/view/SpaceMouseView.cpp app/platform/SpaceMouseDevice.hpp
# check-all sources: app/platform/windows/SpaceMouseWindows.cpp app/framework/TestDriver.cpp tools/ui-scripts/space-mouse.mitcad-ui
# A 3D mouse (mitcad#951) in the build VM, through the test driver's input
# (tools/ui-scripts/space-mouse.mitcad-ui, the same script as the Linux
# test's): no device logs "3D mouse: none" (Raw Input finds none in the
# VM), a spin turns the view, a push pans it, a pull zooms in and the first
# button runs Fit.
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-space-mouse'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/space-mouse.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'space-mouse' @('--ui=qt', '--no-mcp', '--demo')
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail '3D mouse driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3D mouse driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the 3D mouse turned, panned and zoomed the view and its button ran Fit'
Ui-StopApp
Write-Host 'PASS: space-mouse'
