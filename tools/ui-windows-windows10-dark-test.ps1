# SPDX-License-Identifier: MIT
# check-all sources: app/platform/windows/WindowsTheme.cpp app/platform/WindowsTheme.hpp app/main.cpp app/framework/WindowStyle.cpp app/framework/Theme.cpp app/files/StartPage.cpp app/framework/TestDriver.cpp
# Windows 10's dark app mode (mitcad#1282, GitHub mitcad/Mitcad#138) in the
# build VM: the shared steps tools/ui-scripts/windows10-dark.mitcad-ui (the
# Start page dark, light after a switch while Mitcad runs, dark again, then
# the Browser dark), in three starts of MitCad on the Start tab with
# MITCAD_TEST_COLOR_SCHEME=dark:
#   1. windows10-dark-docked: the Windows 10 path forced
#      (MITCAD_TEST_WINDOWS10=1, as the VM runs Windows 11) over Windows
#      10's default style (QT_STYLE_OVERRIDE=windowsvista), which keeps a
#      light palette in the dark app mode: Qt's own style with a dark
#      palette while it is dark, the default style with its own palette
#      again when it is light. Docked.
#   2. windows10-dark-floating: the same in the Floating layout.
#   3. windows11-dark: without the switch, as before: Windows 11's own
#      style follows the app mode, and the path is not taken.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-windows10-dark-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-windows10-dark'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/windows10-dark.mitcad-ui'
$env:MITCAD_TEST_COLOR_SCHEME = 'dark'
$env:MITCAD_START_PAGE = '1'

# One start of MitCad with the shared steps; the app ends with them.
function Run-Theme([string]$Name, [string[]]$Arguments) {
  Ui-StartApp $Name (@('--ui=qt', '--no-mcp') + $Arguments)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: the Start page and the Browser in both app modes"
  Get-Content $script:UiLog | Where-Object { $_ -like '*TestDriver: widget theme*' -or $_ -like '*Windows 10 app mode:*' } |
    ForEach-Object { Write-Host "     $_" }
}

# 1. and 2. The Windows 10 path, Docked and Floating.
$env:MITCAD_TEST_WINDOWS10 = '1'
$env:QT_STYLE_OVERRIDE = 'windowsvista'
foreach ($layout in 'docked', 'floating') {
  $name = "windows10-dark-$layout"
  Run-Theme $name @("--chrome=$layout")
  Ui-ExpectLog 'Windows 10 app mode: followed (Windows build ' "${name}: the Windows 10 path is taken"
  $defaultStyle = Ui-LastMatch 'Windows 10 app mode: followed \(Windows build \d+, MITCAD_TEST_WINDOWS10=1, the style ([^)]+)\)'
  if (-not $defaultStyle) { Ui-Fail "${name}: the default style is not logged" }
  if ($defaultStyle -ne 'windowsvista') { Ui-Fail "${name}: the default style is $defaultStyle, not windowsvista" }
  Ui-ExpectLog 'Windows 10 app mode: dark, the style ' "${name}: Qt's own style in the dark app mode"
  Ui-ExpectLog 'with a dark palette (window #' "${name}: with the dark palette"
  Ui-ExpectLog "Windows 10 app mode: light, the style $defaultStyle with its own palette" `
    "${name}: the default style and its palette again in the light app mode"
  $switches = Ui-CountLog 'Windows 10 app mode: dark, '
  if ($switches -ne 2) { Ui-Fail "${name}: $switches switches to the dark app mode, not 2 (at launch and after the light one)" }
  Ui-StopApp
}
Remove-Item Env:MITCAD_TEST_WINDOWS10 -ErrorAction SilentlyContinue
Remove-Item Env:QT_STYLE_OVERRIDE -ErrorAction SilentlyContinue

# 3. Without the switch: Windows 11's own style, as before.
Run-Theme 'windows11-dark' @('--chrome=docked')
Ui-ExpectLog 'Windows 10 app mode: not followed (Windows build ' 'windows11-dark: the Windows 10 path is not taken'
if ((Ui-CountLog 'Windows 10 app mode: (dark|light), ') -ne 0) { Ui-Fail 'windows11-dark: the style or palette was switched' }
Ui-StopApp
Write-Host 'PASS: the Start page and the Browser follow the app mode on the Windows 10 path; Windows 11 unchanged'
