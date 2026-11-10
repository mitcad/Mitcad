# SPDX-License-Identifier: MIT
# check-all sources: app/framework/UiSizes.cpp app/framework/UiSizes.hpp app/main.cpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriver.cpp app/framework/TestDriverChrome.cpp
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandPanel.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/framework/WindowStyle.cpp tools/ui-scripts/text-icon-size.mitcad-ui
# check-all sources: app/framework/IconSizeStyle.cpp app/framework/IconSizeStyle.hpp
# check-all sources: app/platform/windows/WindowsTheme.cpp
# check-all sources: app/print/MaterialsDialog.cpp
# Preferences > Display > Text size, Icon size and Font (mitcad#1556,
# GitHub mitcad/Mitcad#207) on Windows, the shared input script
# tools/ui-scripts/text-icon-size.mitcad-ui as
# tools/ui-text-icon-size-test.sh runs it on Linux, with the settings
# appearance/textSize, appearance/iconSize and appearance/fontFamily:
# Text size 80 % and Icon size 150 % (Floating), 200 % and 80 % (Floating,
# then Docked), and 120 % and 120 % in the font Arial (Floating). Each time
# the application font is in that family at the platform's size times Text
# size, the Browser's and the ribbon's icons at their sizes times Icon size,
# the icons Qt's style sizes (a menu's and a combo box's, mitcad#1561) at
# the style's sizes times Icon size with the application's style inside Icon
# size's proxy, the window, the ribbon, the Browser and Box's panel lay out whole with no
# input clipped, and
# Icon size one step up in Preferences is saved for the next start.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-text-icon-size-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-text-icon-size'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/text-icon-size.mitcad-ui'
# The sizes alone: no scale factor of the environment's.
Remove-Item Env:QT_SCALE_FACTOR -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_USER_QT_SCALE_FACTOR -ErrorAction SilentlyContinue

# Check-StyleIcons name icon: the icons Qt's style sizes (mitcad#1561): the
# application's style inside Icon size's proxy, and the menu's and the combo
# box's icons in expect-text-icon-size's line at the style's sizes times
# Icon size, rounded to whole pixels (counted here).
function Check-StyleIcons([string]$Name, [int]$Icon) {
  $wrapped = @(Ui-LogLines | Where-Object { $_ -match "Style icon size: \S+ with the style's icons at $Icon%" })
  if ($wrapped.Count -eq 0) { Ui-Fail "$($Name): the application's style is not inside Icon size's proxy" }
  $pattern = "menu icons (\d+)x(\d+) \(the style's (\d+) px\), combo box icons (\d+)x(\d+) \(the style's (\d+) px\)"
  $line = @(Ui-LogLines | Where-Object { $_ -match 'Text and icon size layout: ' }) | Select-Object -Last 1
  $match = [regex]::Match("$line", $pattern)
  if (-not $match.Success) { Ui-Fail "$($Name): no menu and combo box icons in: $line" }
  $menu = [int]$match.Groups[1].Value
  $menuBase = [int]$match.Groups[3].Value
  $combo = [int]$match.Groups[4].Value
  $comboHigh = [int]$match.Groups[5].Value
  $comboBase = [int]$match.Groups[6].Value
  if ([int]$match.Groups[2].Value -ne $menu -or $menu -ne [math]::Floor(($menuBase * $Icon + 50) / 100)) {
    Ui-Fail "$($Name): the menu's icons $menu px, not the style's $menuBase px times $Icon%"
  }
  if ($comboHigh -ne $combo -or $combo -ne [math]::Floor(($comboBase * $Icon + 50) / 100)) {
    Ui-Fail "$($Name): the combo box's icons $($combo)x$comboHigh, not the style's $comboBase px times $Icon%"
  }
  Write-Host "ok   $($Name): menu icons $menu px and combo box icons $combo px (the style's $menuBase and $comboBase px)"
}

# Run-Sizes layout text icon font: one start of MitCad with those settings
# (font 'system': none kept); the app ends with the script's steps.
function Run-Sizes([string]$Layout, [int]$Text, [int]$Icon, [string]$Font) {
  $name = "$Layout, text $Text%, icons $Icon%, font $Font"
  $next = $Icon + 5
  $lines = @('[appearance]', "textSize=$Text", "iconSize=$Icon")
  if ($Font -ne 'system') { $lines += "fontFamily=$Font" }
  Set-Content -Encoding ascii $ini $lines
  $env:MITCAD_TEST_TEXT_SIZE = "$Text"
  $env:MITCAD_TEST_ICON_SIZE = "$Icon"
  $env:MITCAD_TEST_ICON_NEXT = "$next"
  # The Materials window's appearance swatches (mitcad#1610).
  $env:MITCAD_TEST_SWATCH_SIZE = "$([math]::Floor((20 * $Icon + 50) / 100))"
  $env:MITCAD_TEST_FONT = $Font
  $env:MITCAD_TEST_SYSTEM_FONT = if ($Font -eq 'system') { 'true' } else { 'false' }
  Write-Host "--- $name"
  Ui-StartApp ("text-icon-size-$Layout-$Text-$Icon") @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "$($name): the text and icon size driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$($name): the text and icon size driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog "Text and icon size: Text size $Text%, Icon size $Icon%" "$($name): the sizes applied at start"
  Ui-ExpectLog 'Text and icon size layout: font ' "$($name): the font and the Browser's and the ribbon's icons"
  Check-StyleIcons $name $Icon
  Ui-ExpectLog 'Interface size layout: ' "$($name): the window, the ribbon, the Browser and Box's panel whole"
  Ui-ExpectLog 'TestDriver: done' "$($name): Preferences shows the sizes"
  # No label or choice of Box's panel cut at these sizes.
  $clipped = @(Ui-LogLines | Where-Object { $_ -match 'Panel \S+ inputs clipped: ' })
  if ($clipped.Count -gt 0) { Ui-Fail "$($name): a command panel's input is clipped: $($clipped[0])" }
  Ui-ExpectLog 'Panel Box inputs: none clipped' "$($name): no input of Box's panel clipped"
  if (-not (Select-String -Quiet -Path $ini -Pattern "^iconSize=$next$")) {
    Ui-Fail "$($name): Icon size $next% is not saved"
  }
  Write-Host "ok   $($name): Icon size $next% saved for the next start"
  Ui-StopApp
}

Run-Sizes 'floating' 80 150 'system'
Run-Sizes 'floating' 200 80 'system'
Run-Sizes 'docked' 200 80 'system'
Run-Sizes 'floating' 120 120 'Arial'
Write-Host 'PASS: Text size, Icon size and Font'
