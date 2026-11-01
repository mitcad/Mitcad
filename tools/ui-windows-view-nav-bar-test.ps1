# SPDX-License-Identifier: MIT
# check-all sources: app/view/NavigationBar.cpp app/view/NavigationBar.hpp app/MainWindowNavigationBar.cpp
# check-all sources: app/view/ViewController.cpp app/framework/FloatingLayout.cpp app/MainWindow.cpp
# check-all sources: app/OcctViewer.cpp app/sketch/SketchPalette.cpp
# Input-driven Windows UI test of the navigation bar at the bottom of the
# view (mitcad#1086) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), in the Floating and the Docked layout:
#   1. The bar is shown (a card in the Floating layout, in the status bar
#      in the Docked layout) with Look At, Fit, Visual Style, Layout Grid
#      and Snap to Grid; Fit's tooltip gives its shortcut.
#   2. Snap to Grid in the bar and in the View menu stay in step: the bar's
#      button turns the View menu's action off, the View menu's command (by
#      its shortcut, the same action) turns the bar's button on. Layout
#      Grid likewise. The View menu's Wireframe is the bar's style.
#   3. Fit in the bar fits the view.
#   4. In a sketch on XZ, after Isometric View, the bar's Look At looks at
#      the sketch plane (from FRONT, along 0 1 0); the Sketch Palette's
#      Snap to grid is the bar's Snap to Grid.
#   5. Preferences > Display > Show the navigation bar off hides the bar,
#      on shows it again.
# The drop-down's list is a popup, which session 0 closes at once
# (tools/ui-windows-lib.ps1); the Linux counterpart,
# tools/ui-view-nav-bar-test.sh, chooses a style in it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-nav-bar-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-nav-bar'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'view.snap_grid' = 'F8'; 'view.grid' = 'F9';
  'view.style.wireframe' = 'Shift+F5'; 'view.style.visible_edges' = 'Shift+F6'; 'view.iso' = 'F12';
  'tools.preferences.display' = 'Shift+F8' } -StepPause 0

# Expect-States grid snap style description: the bar's last states line, as
# the bar's buttons and the View menu's actions have them.
function Expect-States([string]$Grid, [string]$Snap, [string]$Style, [string]$Description) {
  Ui-ExpectNew "Navigation bar states: Layout Grid $Grid (View menu $Grid), Snap to Grid $Snap (View menu $Snap), Visual Style $Style" $Description
}

# Toggle-Preference on|off: Preferences on its Display page, Show the
# navigation bar (Alt+V), OK.
function Toggle-Preference([string]$Layout, [string]$Shown) {
  Ui-Mark
  Ui-Step 'Preferences: Display (Shift+F8)' { Ui-Key 'shift+F8' }
  Ui-ExpectNew 'Preferences opened: Display, ' "${Layout}: Preferences on its Display page"
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'Show the navigation bar (Alt+V)' { Ui-Key 'alt+v' }
  Ui-Step 'OK (Enter)'                    { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Preferences: navigation bar $Shown" "${Layout}: the preference saved ($Shown)"
}

function Round([string]$Layout, [string]$Chrome, [string]$Place) {
  Write-Host "--- The $Layout layout"
  Ui-StartApp "view-nav-bar-$Chrome" @("--chrome=$Chrome")
  Ui-ExpectLog "Navigation bar shown ($Place) at " "${Layout}: the navigation bar is shown ($Place)"
  foreach ($button in @('Look At', 'Fit', 'Visual Style', 'Layout Grid', 'Snap to Grid')) {
    Ui-ExpectLog "Navigation bar $button at " "${Layout}: the bar has $button"
  }
  if ($Chrome -eq 'floating') {
    Ui-ExpectLog '; navigationBar ' "${Layout}: the bar's card among the Floating cards"
  }
  Ui-ExpectLog '| Fit (F6) |' "${Layout}: Fit's tooltip gives its shortcut"
  $script:UiMark = 0
  Expect-States 'on' 'on' 'Shaded with Visible Edges Only' "${Layout}: the grid and the snap on, the default style"

  Write-Host '--- Snap to Grid, Layout Grid and the style in step with the View menu'
  Ui-Mark
  Ui-Step 'Snap to Grid (bar)'            { Ui-ClickLogged 'Navigation bar Snap to Grid' }
  Ui-ExpectNew 'Snap to grid off' "${Layout}: snap off"
  Expect-States 'on' 'off' 'Shaded with Visible Edges Only' "${Layout}: the View menu's Snap to Grid off with the bar's"
  Ui-Mark
  Ui-Step 'Snap to Grid (View menu command, F8)' { Ui-Key 'F8' }
  Ui-ExpectNew 'Snap to grid on' "${Layout}: snap on from the View menu's command"
  Expect-States 'on' 'on' 'Shaded with Visible Edges Only' "${Layout}: the bar's Snap to Grid on with the View menu's"
  Ui-Mark
  Ui-Step 'Layout Grid (bar)'             { Ui-ClickLogged 'Navigation bar Layout Grid' }
  Ui-ExpectNew 'Layout grid off' "${Layout}: the grid off"
  Expect-States 'off' 'on' 'Shaded with Visible Edges Only' "${Layout}: the View menu's Layout Grid off with the bar's"
  Ui-Mark
  Ui-Step 'Layout Grid (View menu command, F9)' { Ui-Key 'F9' }
  Ui-ExpectNew 'Layout grid on' "${Layout}: the grid on from the View menu's command"
  Expect-States 'on' 'on' 'Shaded with Visible Edges Only' "${Layout}: the bar's Layout Grid on with the View menu's"
  Ui-Mark
  Ui-Step 'Wireframe (View menu command, Shift+F5)' { Ui-Key 'shift+F5' }
  Expect-States 'on' 'on' 'Wireframe' "${Layout}: the bar's style is the View menu's Wireframe"
  Ui-Mark
  Ui-Step 'Shaded with Visible Edges Only (Shift+F6)' { Ui-Key 'shift+F6' }
  Expect-States 'on' 'on' 'Shaded with Visible Edges Only' "${Layout}: the default style again"

  Write-Host '--- Fit'
  Ui-Mark
  Ui-Step 'Fit (bar)'                     { Ui-ClickLogged 'Navigation bar Fit' }
  Ui-ExpectNew 'Navigation bar: Fit' "${Layout}: the bar's Fit clicked"
  Ui-ExpectNew 'Fit: everything shown' "${Layout}: the view fitted"

  Write-Host '--- In a sketch'
  Ui-Mark
  Ui-Step 'create sketch on XZ (F7)'      { Ui-CreateSketch 'F7' 'xz' }
  Ui-ExpectNew 'Sketch plane seen from FRONT (along 0 1 0)' "${Layout}: the XZ sketch seen from FRONT"
  Ui-ExpectLog 'Palette paletteGrid at' "${Layout}: the palette's Snap to grid is laid out"
  Ui-Mark
  Ui-Step 'isometric view (F12)'          { Ui-Key 'F12' }
  Ui-ExpectNew 'Camera direction' "${Layout}: the view turned away from the sketch" 8
  if ((Ui-LastMatch 'Navigation bar (shown|hidden)') -ne 'shown') {
    Ui-Fail "${Layout}: the bar is not shown in the sketch"
  }
  Ui-Mark
  Ui-Step 'Look At (bar)'                 { Ui-ClickLogged 'Navigation bar Look At' }
  Ui-ExpectNew 'Look at sketch' "${Layout}: the bar's Look At looks at the sketch"
  Ui-ExpectNew 'Sketch plane seen from FRONT (along 0 1 0)' "${Layout}: the sketch plane seen from FRONT again"
  Ui-Mark
  Ui-Step 'Snap to grid (palette)'        { Ui-ClickLogged 'Palette paletteGrid' }
  Ui-ExpectNew 'Snap to grid off' "${Layout}: snap off from the palette"
  Expect-States 'on' 'off' 'Shaded with Visible Edges Only' "${Layout}: the bar's Snap to Grid off with the palette's"
  Ui-Mark
  Ui-Step 'Snap to Grid (bar, in the sketch)' { Ui-ClickLogged 'Navigation bar Snap to Grid' }
  Ui-ExpectNew 'Snap to grid on' "${Layout}: snap on from the bar in the sketch"
  Expect-States 'on' 'on' 'Shaded with Visible Edges Only' "${Layout}: snap on again"
  Ui-Mark
  Ui-Step 'finish sketch (Ctrl+Enter)'    { Ui-Key 'ctrl+Return' }
  Ui-ExpectNew 'Sketch finished' "${Layout}: the sketch finished"

  Write-Host '--- The preference'
  Toggle-Preference $Layout 'hidden'
  Ui-ExpectNew 'Navigation bar hidden' "${Layout}: the bar hidden"
  Toggle-Preference $Layout 'shown'
  Ui-ExpectNew "Navigation bar shown ($Place) at " "${Layout}: the bar shown again"
  Ui-StopApp
}

Round 'Floating' 'floating' 'card'
Round 'Docked' 'docked' 'status bar'

Ui-Finish 'UI Windows view navigation bar test'
