# SPDX-License-Identifier: MIT
# check-all sources: app/framework/WindowPlace.cpp app/framework/WindowPlace.hpp app/WindowManager.cpp
# check-all sources: app/DocumentFrame.cpp app/main.cpp app/framework/FitToScreen.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverWindowFit.cpp
# check-all sources: tools/ui-scripts/window-place-save.mitcad-ui tools/ui-scripts/window-place-restore.mitcad-ui
# check-all sources: tools/ui-scripts/window-place-maximized.mitcad-ui tools/ui-scripts/window-place-on-screen.mitcad-ui
# The design window's size, place and state kept between starts
# (mitcad#1710, GitHub mitcad/Mitcad#283) on Windows, the shared input
# scripts tools/ui-scripts/window-place-*.mitcad-ui as
# tools/ui-window-place-test.sh runs them on Linux:
#   1. Nothing kept: the start size; the window resized to 900 x 600 and
#      moved to 70, 50; kept when MitCad ends.
#   2. Started again: the same size and place (its frame included);
#      maximized, kept so.
#   3. Started again: maximized; made normal, the same size and place; a
#      new window at that size, cascaded.
#   4. A kept place on a monitor no longer connected, one half off the
#      screen with its title bar above the top, and one larger than the
#      screen (written into the settings): the whole window within the
#      primary screen's available area.
# In the Floating layout; 1 and 2 also in the Docked one.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-window-place-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-window-place'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
$env:MITCAD_TEST_STEP_MS = '100'

# Run-Place name script layout: one start of MitCad running
# tools/ui-scripts/window-place-<script>.mitcad-ui; the app ends with it.
$script:Runs = 0
function Run-Place([string]$Name, [string]$Script, [string]$Layout) {
  Write-Host "--- $Name"
  $script:Runs++
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/window-place-$Script.mitcad-ui"
  Ui-StartApp ("window-place-$($script:Runs)-$Script-$Layout") @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "$($Name): the window place driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$($Name): the window place driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' $Name
  Ui-LogLines | Where-Object { $_ -match '^(Window place|TestDriver: window )' } | ForEach-Object { Write-Host "     $_" }
  Ui-StopApp
}

# Expect-Kept line description: the settings file has the line.
function Expect-Kept([string]$Line, [string]$Description) {
  $lines = @(Get-Content $ini)
  if ($lines -notcontains $Line) { Ui-Fail "$($Description): '$Line' not in the settings ($($lines -join ' '))" }
  Write-Host "ok   $Description"
}

# Seed-Place geometry description: a kept place written into the settings,
# on no connected screen's area as a whole; MitCad moves it onto the
# primary one.
function Seed-Place([string]$Geometry, [string]$Description) {
  Set-Content -Encoding ascii $ini @('[mainWindow]', "geometry=@Rect($Geometry)", 'screen=gone-monitor', 'maximized=false')
  Run-Place $Description 'on-screen' 'floating'
}

foreach ($layout in @('floating', 'docked')) {
  Set-Content -Encoding ascii $ini @('[shortcuts]')
  Run-Place "$($layout): nothing kept, resized and moved" 'save' $layout
  Ui-ExpectLog 'Window place saved: ' "$($layout): the place kept when MitCad ends"
  Expect-Kept 'maximized=false' "$($layout): kept as a normal window"
  Run-Place "$($layout): started again at the same size and place" 'restore' $layout
  Expect-Kept 'maximized=true' "$($layout): kept as maximized"
}

Run-Place 'floating: started again maximized, normal at the same place' 'maximized' 'floating'

Seed-Place '3000 200 900 600' 'a removed monitor: on the primary screen'
Seed-Place '-450 -20 900 600' 'half off the screen, the title bar above the top: on the screen'
Seed-Place '-100 -100 4000 3000' 'larger than the screen: within it'
Write-Host 'PASS: window place'
