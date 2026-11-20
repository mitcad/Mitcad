# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowDocks.cpp app/framework/DockState.cpp app/framework/DockState.hpp
# check-all sources: app/framework/WindowPlace.cpp app/framework/WindowPlace.hpp app/WindowManager.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowStart.cpp app/framework/CommandDockWidth.cpp
# check-all sources: app/projects/MainWindowProjectsPanel.cpp app/agent/MainWindowChat.cpp
# check-all sources: app/view/ViewController.cpp app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp
# check-all sources: tools/ui-scripts/dock-state-save.mitcad-ui tools/ui-scripts/dock-state-restore.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-render-move.mitcad-ui tools/ui-scripts/dock-state-render-check.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-screen-gone.mitcad-ui tools/ui-scripts/dock-state-default.mitcad-ui
# check-all sources: tools/ui-scripts/dock-state-defaults-kept.mitcad-ui
# check-all sources: app/MainWindowRender.cpp app/framework/Ribbon.cpp
# The Docked layout's docks kept between starts (mitcad#2070, GitHub
# mitcad/Mitcad#451) on Windows, the shared input scripts
# tools/ui-scripts/dock-state-*.mitcad-ui as tools/ui-dock-state-test.sh
# runs them on Linux:
#   1. save: nothing kept; the Render workspace opened and closed, its
#      docks in the places of the docks they hide and those back at their
#      sizes after; the Command panel dragged wider, the Browser
#      docked at the right, the Timeline floated at a place, the Chat panel
#      shown; kept when MitCad ends. The remembered Command width is then
#      set to another one.
#   2. restore, with the Start page at launch: the page hides the docks
#      and shows them again; the Browser at the right, the Timeline
#      floating at its place, View > Panels > Chat checked; the Command
#      panel as wide as when MitCad ended, not at the remembered width.
#   3. render-move: the Render workspace, opened from the Docked layout's
#      workspace selector (mitcad#2082): its docks at their own areas, its list
#      moved; MitCad ends with the workspace open.
#   4. render-check: the docks the workspace hid shown again as kept; the
#      workspace's docks at their own areas again, the selector's Design closes it.
#   5. screen-gone: the kept screen of the floating Timeline renamed to one
#      not connected: the Timeline docked, the log names it.
#   6. default: a new window with the Browser at the right and the Timeline
#      docked; Restore Default Layout puts both windows' docks back at once.
#   7. defaults-kept: the next start opens with the default arrangement.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-dock-state-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-dock-state'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
New-Item -ItemType Directory -Force (Split-Path $ini) | Out-Null
# The Render workspace renders on the processor (as the Linux test).
Set-Content -Encoding ascii $ini @('[shortcuts]', '', '[render]', 'device=cpu')
$env:MITCAD_TEST_STEP_MS = '100'

# Run-Docks name script: one Docked start of MitCad running
# tools/ui-scripts/dock-state-<script>.mitcad-ui; the app ends with it.
$script:Runs = 0
function Run-Docks([string]$Name, [string]$Script) {
  Write-Host "--- $Name"
  $script:Runs++
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/dock-state-$Script.mitcad-ui"
  Ui-StartApp ("dock-state-$($script:Runs)-$Script") @('--ui=qt', '--no-mcp', '--chrome=docked')
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "$($Name): the dock state driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$($Name): the dock state driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' $Name
  Ui-LogLines | Where-Object { $_ -match '^(Dock state|TestDriver: dock )' } | ForEach-Object { Write-Host "     $_" }
}

# The Command panel's width last logged in this start.
function Command-Width {
  $width = ''
  foreach ($line in @(Ui-LogLines)) {
    $match = [regex]::Match($line, '^Command panel width (\d+) ')
    if ($match.Success) { $width = $match.Groups[1].Value }
  }
  return $width
}

Run-Docks '1. nothing kept: docks moved, floated and shown' 'save'
Ui-ExpectLog 'Dock state saved: ' '1. the docks kept when MitCad ends'
$lines = @(Get-Content $ini)
if (-not ($lines -match '^\[dockState\]')) { Ui-Fail "1. no [dockState] in the settings" }
if (-not ($lines -match '^floating.timelineDock.screen=')) { Ui-Fail "1. the floating Timeline's screen is not kept" }
if (-not ($lines -contains 'dockWidth=560')) { Ui-Fail '1. the remembered Command width was not set to 560' }
Write-Host "ok   1. kept in the settings, the floating Timeline's frame and screen with it"
$width = Command-Width
if (-not $width) { Ui-Fail '1. no Command panel width logged' }
Ui-StopApp

$env:MITCAD_START_PAGE = '1'
Run-Docks '2. restored, behind the Start page and after it' 'restore'
$env:MITCAD_START_PAGE = '0'
$restored = Command-Width
if (-not $restored -or [Math]::Abs([int]$restored - [int]$width) -gt 2) {
  Ui-Fail "2. the Command panel is $restored px wide after the restart, not $width (the remembered 560 applied?)"
}
Write-Host "ok   2. the Command panel is $restored px wide again ($width when MitCad ended)"
Ui-StopApp

Run-Docks "3. the Render workspace's docks, one moved, MitCad ends in it" 'render-move'
Ui-ExpectLog 'shown at the next start: ' '3. the docks the workspace hid kept as shown'
Ui-StopApp
Run-Docks '4. the hidden docks shown again, the workspace''s at their own areas' 'render-check'
Ui-StopApp

$renamed = @(Get-Content $ini) | ForEach-Object { $_ -replace '^(floating.timelineDock.screen=).*$', '${1}gone-monitor' }
Set-Content -Encoding ascii $ini $renamed
if (-not (@(Get-Content $ini) -contains 'floating\timelineDock\screen=gone-monitor')) { Ui-Fail '5. the kept screen was not renamed' }
Run-Docks "5. the floating Timeline's screen gone: docked" 'screen-gone'
Ui-StopApp
Run-Docks '6. a new window, Restore Default Layout in both' 'default'
Ui-StopApp
Run-Docks '7. the default arrangement at the next start' 'defaults-kept'
Ui-StopApp
if (@(Get-Content $ini) -match '^floating.timelineDock') { Ui-Fail '7. a floating Timeline is still kept after Restore Default Layout' }
Write-Host 'ok   7. nothing floating kept'
Write-Host 'PASS: Docked dock state'
