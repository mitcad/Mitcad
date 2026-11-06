# SPDX-License-Identifier: MIT
# check-all sources: app/framework/AppTheme.cpp app/framework/AppTheme.hpp app/framework/Theme.cpp app/platform/windows/WindowsTheme.cpp app/platform/WindowsTheme.hpp app/main.cpp app/view/ViewController.cpp app/framework/TestDriver.cpp
# Preferences > Display > Theme (mitcad#1330, GitHub mitcad/Mitcad#151) in
# the build VM, as tools/ui-theme-preference-test.sh on Linux: the shared
# steps tools/ui-scripts/theme-preference.mitcad-ui. On the VM's light app
# mode, MitCad started with the setting appearance/theme=dark shows the
# Start page, the Browser's card, the Browser and the timeline dark; Light
# chosen in Preferences repaints them light without a restart, Dark dark
# again, and System follows the app mode again (light, then its switch to
# dark and back). Three starts:
#   1. windows11-floating: Windows 11's own style, Floating.
#   2. windows11-docked: the same, Docked.
#   3. windows10-floating: Windows 10's path forced (MITCAD_TEST_WINDOWS10=1
#      over QT_STYLE_OVERRIDE=windowsvista, whose palette stays light): Qt's
#      own style with the dark palette while the requested scheme is dark,
#      the default style with its own palette for Light and System.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-theme-preference-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-theme-preference'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
Remove-Item Env:MITCAD_TEST_COLOR_SCHEME -ErrorAction SilentlyContinue
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/theme-preference.mitcad-ui'
$env:MITCAD_START_PAGE = '1'
$env:MITCAD_TEST_THEME_DESKTOP_DARK = 'dark'
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
$baseIni = @(Get-Content $ini)

# One start of MitCad with the Theme preference Dark stored and the shared
# steps; the app ends with them.
function Run-Theme([string]$Name, [string[]]$Arguments) {
  Set-Content -Encoding ascii $ini ($baseIni + @('[appearance]', 'theme=dark'))
  Ui-StartApp $Name (@('--ui=qt', '--no-mcp') + $Arguments)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: dark at launch, light and dark from Preferences, System follows the app mode"
  Get-Content $script:UiLog | Where-Object {
    $_ -like '*TestDriver: widget theme*' -or $_ -like '*Theme: *' -or $_ -like '*Windows 10 app mode:*'
  } | ForEach-Object { Write-Host "     $_" }
  if (-not ((Get-Content $ini) -contains 'theme=system')) { Ui-Fail "${Name}: System is not saved as appearance/theme" }
  Write-Host "ok   ${Name}: saved as appearance/theme"
}

# 1. and 2. Windows 11's own style.
foreach ($layout in 'floating', 'docked') {
  $name = "windows11-$layout"
  Run-Theme $name @("--chrome=$layout")
  Ui-ExpectLog 'Windows 10 app mode: not followed (Windows build ' "${name}: the Windows 10 path is not taken"
  Ui-StopApp
}

# 3. The Windows 10 path.
$env:MITCAD_TEST_WINDOWS10 = '1'
$env:QT_STYLE_OVERRIDE = 'windowsvista'
$name = 'windows10-floating'
Run-Theme $name @('--chrome=floating')
Ui-ExpectLog 'Windows 10 app mode: followed (Windows build ' "${name}: the Windows 10 path is taken"
Ui-ExpectLog 'Windows 10 app mode: dark, the style ' "${name}: Qt's own style for the dark scheme asked"
Ui-ExpectLog 'Windows 10 app mode: light, the style windowsvista with its own palette' `
  "${name}: the default style and its palette again for Light"
Ui-StopApp
Remove-Item Env:MITCAD_TEST_WINDOWS10 -ErrorAction SilentlyContinue
Remove-Item Env:QT_STYLE_OVERRIDE -ErrorAction SilentlyContinue
Write-Host 'PASS: the Theme preference: dark at launch, Light and Dark at once, System follows the app mode; Windows 11 and Windows 10 paths'
