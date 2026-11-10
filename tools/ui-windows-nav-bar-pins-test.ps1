# SPDX-License-Identifier: MIT
# check-all sources: app/view/NavigationBar.cpp app/view/NavigationBar.hpp app/MainWindowNavigationBar.cpp
# check-all sources: app/framework/NavigationBarPins.cpp app/framework/Ribbon.cpp app/MainWindow.cpp
# Any command pinned to the navigation bar (mitcad#1589, GitHub
# mitcad/Mitcad#224) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), in the Floating and then the Docked layout, each round
# with the default pins:
#   1. The bar has Edit Parameters pinned by default; the preference
#      (navigationBarPins) is not written yet.
#   2. Measure, the first row of the SOLID > INSPECT menu, is pinned from
#      its row's menu by the keys (the Menu key; Pin to Toolbar is
#      unavailable, Measure has its button, so Pin to Navigation Bar is
#      chosen): the bar has Measure after Edit Parameters, saved.
#   3. In the BUILD workspace a click on the bar's Measure starts Measure.
#   4. Measure dragged before Edit Parameters takes the first place.
#   5. Restarted with the same settings: Measure and Edit Parameters, in
#      that order.
#   6. Measure's right-click on the bar: Unpin from Navigation Bar and
#      Change Keyboard Shortcut (chosen by the keys: session 0 posts its
#      clicks to the main window, not to a menu's own window); Unpin from
#      Navigation Bar leaves Edit Parameters.
# The Linux counterpart, tools/ui-nav-bar-pins-test.sh, also opens Keyboard
# Shortcuts from the bar and unpins and pins Edit Parameters from its row.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-nav-bar-pins-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-nav-bar-pins'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0

# The settings' INI file of the test's own (Ui-Init).
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'

# Pins-Saved ids description: the settings' navigationBarPins is the ids
# ('<none>' without the key).
function Pins-Saved([string]$Ids, [string]$Description) {
  $value = '<none>'
  foreach ($line in @(Get-Content $ini)) {
    if ($line.StartsWith('navigationBarPins=')) { $value = $line.Substring(18).Replace('"', '').Replace(' ', '') }
  }
  if ($value -ne $Ids) { Ui-Fail "${Description}: navigationBarPins is '$value', not '$Ids'" }
  Write-Host "ok   ${Description}: navigationBarPins '$Ids'"
}

# X-Of "text": the x of the last place logged as "<text> at x,y".
function X-Of([string]$Text) { return (Ui-LoggedAt $Text)[0] }

function Round([string]$Layout, [string]$Chrome, [string]$Place) {
  Write-Host "--- The $Layout layout"
  # The default pins again: the preference out of the settings.
  Set-Content -Encoding ascii $ini @(Get-Content $ini | Where-Object { -not $_.StartsWith('navigationBarPins=') })

  # 1. Edit Parameters pinned by default.
  Ui-StartApp "nav-bar-pins-$Chrome" @("--chrome=$Chrome")
  Ui-ExpectLog "Navigation bar shown ($Place) at " "${Layout}: the navigation bar is shown ($Place)"
  Ui-ExpectLog 'Navigation bar pinned: solid.parameters' "${Layout}: Edit Parameters pinned by default"
  Ui-ExpectLog 'Navigation bar Edit Parameters at ' "${Layout}: Edit Parameters on the bar"
  Pins-Saved '<none>' "${Layout}: the default is not written"

  # 2. Measure pinned from its row in SOLID > INSPECT, by the keys.
  Ui-Step 'the SOLID tab'                  { Ui-ClickLogged 'Ribbon tab SOLID' }
  Ui-Mark
  Ui-Step 'the INSPECT menu'               { Ui-ClickLogged 'Ribbon SOLID/INSPECT' }
  Ui-ExpectNew 'Ribbon menu SOLID/INSPECT: Measure' "${Layout}: the INSPECT menu opened, Measure first"
  Ui-Step 'Measure''s row, the Menu key'    { Ui-Key 'Down', 'Menu' }
  Ui-ExpectNew 'Toolbar menu SOLID/INSPECT inspect.measure: Pin to Toolbar | Pin to Navigation Bar | Change Keyboard Shortcut' "${Layout}: Measure's row menu"
  Ui-Step 'Pin to Navigation Bar (Enter)'  { Ui-Key 'Return' }
  Ui-ExpectNew 'Pinned to navigation bar: Measure' "${Layout}: Measure pinned"
  Ui-ExpectNew 'Navigation bar pinned: solid.parameters, inspect.measure' "${Layout}: Measure after Edit Parameters"
  Ui-Sync
  Ui-ExpectNew 'Navigation bar Measure at ' "${Layout}: Measure's button on the bar"
  if ((X-Of 'Navigation bar Edit Parameters') -ge (X-Of 'Navigation bar Measure')) {
    Ui-Fail "${Layout}: Measure not right of Edit Parameters"
  }
  Pins-Saved 'solid.parameters,inspect.measure' "${Layout}: saved"

  # 3. Measure from the bar in the BUILD workspace.
  Ui-Mark
  Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
  Ui-ExpectNew 'Ribbon tab shown: BUILD' "${Layout}: the BUILD workspace"
  Ui-Mark
  Ui-Step 'Measure (bar)'                  { Ui-ClickLogged 'Navigation bar Measure' }
  Ui-ExpectNew 'Navigation bar: Measure' "${Layout}: the bar's Measure clicked"
  Ui-ExpectNew 'Command Measure started' "${Layout}: Measure started in BUILD"
  Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
  Ui-ExpectNew 'Command Measure cancelled' "${Layout}: Measure cancelled"

  # 4. Measure dragged before Edit Parameters.
  Ui-Mark
  $from = Ui-LoggedAt 'Navigation bar Measure'
  $to = Ui-LoggedAt 'Navigation bar Edit Parameters'
  Ui-Step 'drag Measure before Edit Parameters' { Ui-Drag $from[0] $from[1] ($to[0] - 12) $to[1] }
  Ui-ExpectNew 'Navigation bar pinned order: Measure, Edit Parameters' "${Layout}: Measure dragged first"
  Ui-ExpectNew 'Navigation bar pinned: inspect.measure, solid.parameters' "${Layout}: the bar in the new order"
  Pins-Saved 'inspect.measure,solid.parameters' "${Layout}: the new order saved"
  if (Ui-NewLines | Where-Object { $_.Contains('Command Measure started') }) { Ui-Fail "${Layout}: the drag started Measure" }
  Write-Host "ok   ${Layout}: the drag started no command"
  Ui-StopApp

  # 5. Restarted with the same settings.
  Ui-StartApp "nav-bar-pins-$Chrome-again" @("--chrome=$Chrome")
  Ui-ExpectLog 'Navigation bar pinned: inspect.measure, solid.parameters' "${Layout}: the pins read back after a restart"
  Ui-ExpectLog 'Navigation bar Measure at ' "${Layout}: Measure on the bar after a restart"
  Ui-Sync
  if ((X-Of 'Navigation bar Measure') -ge (X-Of 'Navigation bar Edit Parameters')) {
    Ui-Fail "${Layout}: Measure not left of Edit Parameters after a restart"
  }
  Write-Host "ok   ${Layout}: Measure before Edit Parameters after a restart"

  # 6. Measure's right-click on the bar: its menu, Unpin from Navigation Bar
  # (its first entry) by the keys.
  Ui-Mark
  Ui-Step 'right-click Measure (bar)'      { Ui-ClickLogged 'Navigation bar Measure' 3 }
  Ui-ExpectNew 'Navigation bar menu inspect.measure: Unpin from Navigation Bar | Change Keyboard Shortcut' "${Layout}: the pinned button's menu"
  Ui-Step 'Unpin from Navigation Bar (Down, Enter)' { Ui-Key 'Down', 'Return' }
  Ui-ExpectNew 'Unpinned from navigation bar: Measure' "${Layout}: Measure unpinned"
  Ui-ExpectNew 'Navigation bar pinned: solid.parameters' "${Layout}: Edit Parameters left"
  Pins-Saved 'solid.parameters' "${Layout}: Measure no longer saved"
  Ui-StopApp
}

Round 'Floating' 'floating' 'card'
Round 'Docked' 'docked' 'status bar'

Ui-Finish 'UI Windows navigation bar pins test'
