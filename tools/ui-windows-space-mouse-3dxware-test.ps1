# SPDX-License-Identifier: MIT
# check-all sources: app/platform/windows/SpaceMouse3DxWare.cpp app/platform/windows/SpaceMouseWindows.cpp
# check-all sources: app/platform/SpaceMouseDevice.hpp app/view/SpaceMouseController.cpp app/MainWindow.cpp
# check-all sources: app/DocumentFrame.cpp app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp
# check-all sources: app/framework/TestDriverNativeWindows.cpp tools/ui-scripts/space-mouse-3dxware.mitcad-ui
# The 3DxWare driver on Windows (mitcad#2152, GitHub mitcad/Mitcad#529) in
# the build VM, through the test's stand-in for its runtime
# (MITCAD_TEST_3DXWARE=1: one device, "Test 3D mouse"), in the Floating
# layout, with the script tools/ui-scripts/space-mouse-3dxware.mitcad-ui:
# the driver is given only a window that exists, the design window has no
# native window of its own (which painted the Floating cards' translucent
# margins black), the screen beside the Browser's card, the navigation bar
# and the message band shows the view's background, and the driver
# connects ("via 3DxWare") once the design sits in its shown frame.
# Session 0 draws no window (Qt never exposes it there, ui-windows-lib.ps1):
# there the screen is not checked, which the log says; the native window is.
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-space-mouse-3dxware'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_3DXWARE = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/space-mouse-3dxware.mitcad-ui'
Ui-StartApp 'space-mouse-3dxware' @('--ui=qt', '--no-mcp', '--demo', '--chrome=floating')
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '3DxWare driver test timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "3DxWare driver test failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the script ran to its end'
Ui-ExpectLog '3D mouse: 3DxWare connected (Test 3D mouse via 3DxWare)' 'the driver connected through 3DxWare, not only Raw Input'
Ui-ExpectLog 'Design native windows: none' 'the design window has no native window of its own'
if ((Ui-LogText).Contains('Screen beside cards: not drawn here, not checked')) {
  Write-Host 'note: the system draws no window here (session 0): the screen beside the cards was not checked'
} else {
  Ui-ExpectLog 'Screen beside cards: beside ' "the screen beside the Floating cards shows the view's background"
}
Ui-StopApp
Write-Host 'PASS: space-mouse-3dxware'
