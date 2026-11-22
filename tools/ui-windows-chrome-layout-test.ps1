# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ChromeStyle.cpp app/framework/ChromeStyle.hpp app/framework/Restart.cpp
# check-all sources: app/framework/Restart.hpp app/framework/FloatingLayout.cpp app/framework/FloatingLayout.hpp
# check-all sources: app/framework/GlassCard.cpp app/framework/TitleBar.cpp app/framework/Ribbon.cpp
# check-all sources: app/main.cpp app/view/ViewController.cpp app/MainWindow.cpp app/OcctViewer.cpp tools/cli
# check-all sources: app/browser/BrowserBackground.cpp app/DocumentFrame.cpp app/framework/RibbonLabels.cpp
# check-all sources: app/MainWindowOrigin.cpp app/MainWindowSketch3d.cpp
# Input-driven Windows UI test of the window's layout chosen in Preferences
# (mitcad#419) and of the defaults (mitcad#429) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-chrome-layout-test.sh does on Linux:
#   1. Without a choice (a plate open) Floating with the Browser's Rows
#      (mitcad#2144; the card from mitcad#1990 until then):
#      nothing above the view, no menu bar under the system's title bar
#      (mitcad#1990: its menus are the title row's); the title row (one flat
#      bar across the view's top, mitcad#1990: its controls, each whole
#      within it: Home, File and Save, Undo and Redo, the document tabs,
#      Preferences and Help; no title box, and no quick-access group: it
#      has no commands by default, mitcad#1990) and the
#      toolbar panel (the workspace selector, the workspace's tabs, the
#      search field and at least two groups within it, mitcad#1990) are over
#      its top, every other card starts below them; the Browser's card
#      overlaps none of the left column's other cards (Projects,
#      Configuration) nor the Timeline; no place between two
#      cards is logged, and with the plate zoomed in to fill the view, a
#      click on the bar's free stretch stays the bar's (nothing selected).
#      Preferences > Display > Layout Docked asks Restart MitCad, Later
#      keeps it in the settings for the next start.
#   2. Floating again is the window's layout: saved, no question.
#   3. A sketch on XY clicked in the view as a user would: Create Sketch's
#      command card leaves XY's place clear, and the Browser's card on its
#      card (with Rows the click beside the rows' boxes reaches the view,
#      mitcad#419) (mitcad#1990: the view moves the origin's planes out from
#      under them); the next sketch on XY picked from the Browser. Docked,
#      Restart Now with unsaved changes: the window asks about them as
#      quitting does; Cancel keeps Mitcad open and nothing restarts.
#   4. Saved, Docked, Restart Now: Mitcad quits and would start again
#      (MITCAD_TEST_NO_RESTART=1 logs the command) with the run's settings
#      options and the design, without --chrome.
#   5. The next start is Docked by the preference: no cards; Preferences
#      shows the Browser background there too (mitcad#429); Floating and
#      the Card background chosen together, Later.
#   6. The next start is Floating by the preference, with the Card
#      background as chosen (mitcad#2144: a saved Card stays Card): the
#      cards at the default size, in a narrow and a wide window; a click on
#      the title bar beside its controls stays the bar's.
#   7. --chrome=docked overrides the preference.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-chrome-layout-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-chrome-layout'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The window resized as a user would drag its frame (the outer size).
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

public static class LayoutTestWindow {
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hwnd, IntPtr after, int x, int y, int cx, int cy,
                                                               uint flags);
    const uint SWP_NOMOVE = 0x2, SWP_NOZORDER = 0x4, SWP_NOACTIVATE = 0x10;

    public static bool Resize(IntPtr hwnd, int width, int height) {
        return SetWindowPos(hwnd, IntPtr.Zero, 0, 0, width, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
'@

# Popups close at once in session 0: the Display page by a key of the
# test's own; Create Sketch by its default Alt+S. The top view by F7.
Ui-Init $App $Out $QtBin @{ 'tools.preferences.display' = 'F8'; 'view.top' = 'F7' } -StepPause 0

$work = Join-Path $script:UiOut 'chrome-layout'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$file = Join-Path $work 'part.mitcad'
$fileLogged = $file -replace '\\', '/'
# A 200 x 200 x 10 mm plate centred on the origin: Body1 (F2.b0), for the
# clicks on the title row beside its controls.
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$plateScript = Join-Path $work 'plate.json'
$plate = Join-Path $work 'plate.mitcad'
Set-Content -Encoding ascii $plateScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
'@
$output = & $cli run $plateScript --save $plate
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $plate)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
# The restart is logged, not started: the test starts the next run itself.
$env:MITCAD_TEST_NO_RESTART = '1'

# Choose-Layout key [background]: Preferences on its Display page (F8), the
# Layout (Alt+L) set with the key (Home: Docked, End: Floating), the Browser
# background (Alt+B) with the other key if given (Home: Card, End: Rows), OK.
function Choose-Layout([string]$Key, [string]$Background = '') {
  Ui-Mark
  Ui-Step 'Preferences: Display (F8)'      { Ui-Key 'F8' }
  Ui-ExpectNew 'Preferences opened: Display, ' 'Preferences on its Display page'
  Ui-FocusDialog '^Preferences$'
  Ui-Step "Layout (Alt+L, $Key)"           { Ui-Key 'alt+l'; Ui-Key $Key }
  if ($Background) {
    Ui-ExpectNew 'Preferences browser background at ' 'the Browser background shown on the Display page'
    Ui-Step "Browser background (Alt+B, $Background)" { Ui-Key 'alt+b'; Ui-Key $Background }
  }
  Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
  Ui-FocusMain
}

function Has-Line([string]$Text) { return [bool]((Ui-LogLines) | Where-Object { $_.Contains($Text) }) }

# Cards-BelowBar description min max: the last "Floating cards:" line, of a
# window min to max wide (waited for): nothing above the view, the menu bar
# no higher than nothing (its bottom the view's top, 0; mitcad#1990: no
# menu bar row under the system's title bar); the title row, a bar from the
# view's left edge to its right at the window's top (mitcad#1990), with its controls, each whole within it: Home, File and
# Save, Undo and Redo, the document tabs, Preferences and Help, without a
# title box and without the quick-access group (no commands by default,
# mitcad#1990); below it the toolbar panel (mitcad#1990) with the
# workspace selector at its left, the workspace's tabs, the search field
# and at least two groups within it, all below the bar; the Browser's card
# and every other card start below the floating rows' bottom. Returns the
# line's entries by name: x, y, width, height.
function Cards-BelowBar([string]$Description, [int]$Min, [int]$Max) {
  $line = $null
  $width = -1
  # Looked for every 50 ms (200 ms before, mitcad#870), up to 5 s.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $line = "$(Ui-LastLineWith 'Floating cards: ' -Start)"
    if ($line -match '^Floating cards: window (\d+)x') { $width = [int]$Matches[1] }
    if ($width -ge $Min -and $width -le $Max) { break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 5000)
  if ($width -lt $Min -or $width -gt $Max) {
    Ui-Fail "${Description}: no Floating cards line of a window $Min to $Max wide (last: '$line')"
  }
  if ($line -notmatch '^Floating cards: window \d+x\d+, top bar bottom (\d+), view top (\d+), floating rows bottom (\d+); (.*)$') {
    Ui-Fail "${Description}: '$line'"
  }
  $bar = [int]$Matches[1]
  $view = [int]$Matches[2]
  $rows = [int]$Matches[3]
  $rest = $Matches[4]
  if ($bar -ne 0) { Ui-Fail "${Description}: a menu bar row above the view in '$line'" }
  if ($view -ne $bar) { Ui-Fail "${Description}: the view's top $view is not the window's top $bar ($line)" }
  if ($rows -le $bar) { Ui-Fail "${Description}: no floating rows below the bar ($line)" }
  if (-not $rest.Contains('browserDock')) { Ui-Fail "${Description}: no Browser card in '$line'" }
  $entries = @{}
  $count = 0
  $groups = 0
  foreach ($entry in ($rest -split ';')) {
    if ($entry.Trim() -notmatch '^(\S+) (-?\d+),(-?\d+) (\d+)x(\d+)$') { continue }
    $name = $Matches[1]
    $y = [int]$Matches[3]
    $entries[$name] = @([int]$Matches[2], $y, [int]$Matches[4], [int]$Matches[5])
    if ($y -lt $bar) { Ui-Fail "${Description}: ${name}'s top $y is above the top bar's bottom $bar ($line)" }
    if ($name.StartsWith('ribbonGroup_')) { $groups++ }
    elseif (-not $name.StartsWith('ribbon') -and $name -ne 'noticeCard' -and -not $name.StartsWith('title') -and $y -lt $rows) {
      Ui-Fail "${Description}: ${name}'s top $y is above the floating rows' bottom $rows ($line)"
    }
    $count++
  }
  if (-not $entries.ContainsKey('titleRow')) { Ui-Fail "${Description}: no title row in '$line'" }
  if ($entries.ContainsKey('titleCard')) { Ui-Fail "${Description}: a title card in '$line'" }
  if ($entries.ContainsKey('titleFinishSketch')) { Ui-Fail "${Description}: duplicate Finish Sketch in the title row: '$line'" }
  if ($entries.ContainsKey('titleSearch')) { Ui-Fail "${Description}: the search field in the title row, not the panel: '$line'" }
  foreach ($part in @('titleQuickAccess', 'titleMenu', 'titleTabs', 'titleSettings', 'ribbon', 'ribbonWorkspace', 'ribbonTabs', 'ribbonSearch')) {
    if (-not $entries.ContainsKey($part)) { Ui-Fail "${Description}: no $part in '$line'" }
  }
  # The title row is a bar across the view at the window's top.
  $bar0 = $entries['titleRow']
  if ($bar0[0] -gt 1 -or $bar0[1] -ne $bar -or $bar0[2] -lt $width - 2) {
    Ui-Fail "${Description}: the title row ($($bar0 -join ',')) is not a bar across the window at ${bar}: '$line'"
  }
  # The workspace selector at the panel's left, the tabs right of it.
  $selector = $entries['ribbonWorkspace']
  if ($selector[0] -ne $entries['ribbon'][0] -or $entries['ribbonTabs'][0] -le $selector[0] + $selector[2]) {
    Ui-Fail "${Description}: the workspace selector ($($selector -join ',')) is not at the panel's left before the tabs: '$line'"
  }
  # The panel's parts within its card, which starts below the title row.
  $panel = $entries['ribbon']
  $title = $entries['titleRow']
  if ($panel[1] -lt $title[1] + $title[3]) { Ui-Fail "${Description}: the toolbar panel ($($panel[1])) is not below the title row: '$line'" }
  foreach ($name in @($entries.Keys | Where-Object { $_ -eq 'ribbonWorkspace' -or $_ -eq 'ribbonTabs' -or $_ -eq 'ribbonSearch' -or $_.StartsWith('ribbonGroup_') })) {
    $part = $entries[$name]
    if ($part[0] -lt $panel[0] -or $part[1] -lt $panel[1] -or $part[0] + $part[2] -gt $panel[0] + $panel[2] -or
        $part[1] + $part[3] -gt $panel[1] + $panel[3]) {
      Ui-Fail "${Description}: $name ($($part -join ',')) is not within the toolbar panel ($($panel -join ',')): '$line'"
    }
  }
  # Every control of the row whole within the row.
  $row = $entries['titleRow']
  foreach ($name in @($entries.Keys | Where-Object { $_.StartsWith('title') -and $_ -ne 'titleRow' })) {
    $part = $entries[$name]
    if ($part[0] -lt $row[0] -or $part[1] -lt $row[1] -or $part[0] + $part[2] -gt $row[0] + $row[2] -or
        $part[1] + $part[3] -gt $row[1] + $row[3]) {
      Ui-Fail "${Description}: $name ($($part -join ',')) is not within the title row ($($row -join ',')): '$line'"
    }
  }
  if ($groups -lt 2) { Ui-Fail "${Description}: $groups groups in the toolbar panel in '$line'" }
  Ui-QuickAccessPlace $Description $line
  # The Browser's card (Rows or Card, mitcad#2144) overlaps none of the left
  # column's other cards nor the Timeline.
  $browser = $entries['browserDock']
  foreach ($name in @('projectsDock', 'configurationDock', 'timelineDock')) {
    if (-not $entries.ContainsKey($name)) { continue }
    $other = $entries[$name]
    if ($other[0] -lt $browser[0] + $browser[2] -and $browser[0] -lt $other[0] + $other[2] -and
        $other[1] -lt $browser[1] + $browser[3] -and $browser[1] -lt $other[1] + $other[3]) {
      Ui-Fail "${Description}: the Browser's card ($($browser -join ',')) overlaps $name ($($other -join ',')): '$line'"
    }
  }
  Write-Host "ok   ${Description}: the view below the menu bar ($bar), $count cards and parts, $groups groups in the toolbar panel, the title row's and the panel's parts within them, the others below the rows' bottom $rows (window $width wide)"
  return $entries
}

# Zoom-Plate: the plate seen from the top, zoomed in until it fills the view
# (also behind the floating rows).
function Zoom-Plate {
  Ui-Step 'top view (F7)'                  { Ui-Key 'F7' }
  Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
  # The fitted view's places, as the Linux test syncs once more.
  Ui-Sync
  $plate = Ui-LoggedAt 'Body F2.b0'
  Ui-Step 'zoom in at the plate''s middle' {
    [UiInput]::Move($script:UiWindow, $plate[0], $plate[1], 0)
    for ($i = 0; $i -lt 14; $i++) {
      Start-Sleep -Milliseconds 60
      [UiInput]::Wheel($script:UiWindow, $plate[0], $plate[1], 1)
    }
  }
  Ui-WaitIdle
}

# No-Gaps: the floating rows take the mouse on their whole width (the title
# row a bar, the toolbar panel's parts one card, mitcad#1990): no place
# between two cards is logged.
function No-Gaps {
  if (Has-Line 'Floating view between the cards at') { Ui-Fail 'a place between two cards of the floating rows' }
  Write-Host "ok   no gap between the floating rows' cards"
}

# Click-TitleRow entries: with the plate filling the view and nothing
# selected, a click on the title bar between the document tabs and the next
# part stays the bar's (mitcad#1990: one flat bar): the plate's face is not
# selected.
function Click-TitleRow($Entries) {
  Zoom-Plate
  $row = $Entries['titleRow']
  $tabs = $Entries['titleTabs']
  $next = $Entries.GetEnumerator() | Where-Object { $_.Key.StartsWith('title') -and $_.Key -ne 'titleRow' -and
    $_.Value[0] -gt $tabs[0] } | Sort-Object { $_.Value[0] } | Select-Object -First 1
  if (-not $next) { Ui-Fail 'nothing right of the title row''s document tabs' }
  $cx = [int](($tabs[0] + $tabs[2] + $next.Value[0]) / 2)
  $cy = $row[1] + [int]($row[3] / 2)
  Ui-Mark
  Ui-Step "click the title bar beside its controls ($cx,$cy)" { Ui-Click $cx $cy }
  Start-Sleep -Milliseconds 500
  if (@(Ui-NewLines | Where-Object { $_.Contains('Selected: 1 face [face F2:') }).Count -gt 0) {
    Ui-Fail "the click on the title bar reached the view: the plate's face selected"
  }
  Write-Host "ok   the click on the title bar stayed the bar's"
}

# Sketch-OnXyInView: Create Sketch (Alt+S) on XY clicked in the view, as a
# user would. In the Floating layout the view of a new design is framed
# beside the Browser's card (mitcad#1990), and Create Sketch's own card (its
# 2D and 3D row, mitcad#1524) came over the only part of XY's square left
# to click; the view now moves the origin's planes out from under the cards
# as the pick starts ("View for the origin planes: moved ..."). XY's logged
# place is inside neither the command card nor, on its card, the Browser's
# card of the last "Floating cards:" line. With the Rows background
# (mitcad#2144: the default) the Browser takes the mouse only on its rows'
# boxes (mitcad#419), and the view keeps the planes clear of their extent
# (GlassCard::coveredRect), not of the card's rectangle: a place in that
# rectangle beside the rows reaches the view, which the click then shows
# (the sketch starts on XY). As sketch_on_xy_in_view in the Linux twin.
function Sketch-OnXyInView {
  $cards = @('commandDock', 'browserDock')
  if ("$(Ui-LastLineWith 'Browser background: ' -Start)" -eq 'Browser background: rows') { $cards = @('commandDock') }
  Ui-Mark
  Ui-Key 'alt+s'
  Ui-ExpectNew 'Create Sketch: select a plane' 'Create Sketch asks for a plane' | Out-Null
  Ui-SyncOr 500
  # After a move, XY's place where the view came to rest.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $lines = @(Ui-NewLines)
    $moved = -1
    for ($i = $lines.Count - 1; $i -ge 0; $i--) {
      if ($lines[$i].StartsWith('View for the origin planes: moved')) { $moved = $i; break }
    }
    if ($moved -lt 0) { break }
    $placed = $false
    for ($i = $moved + 1; $i -lt $lines.Count; $i++) {
      if ($lines[$i].StartsWith('Datum xy at ')) { $placed = $true; break }
    }
    if ($placed) { Write-Host "ok   $($lines[$moved])"; break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 5000)
  $line = "$(Ui-LastLineWith 'Floating cards: ' -Start)"
  $at = Ui-LastMatch '^Datum xy at (-?\d+,-?\d+)$'
  if (-not $at) { Ui-Fail 'no place of XY logged for Create Sketch' }
  $xy = $at -split ','
  $ax = [int]$xy[0]
  $ay = [int]$xy[1]
  foreach ($name in $cards) {
    $rect = $null
    foreach ($entry in ($line -split ';')) {
      if ($entry.Trim() -match ('^' + $name + ' (-?\d+),(-?\d+) (\d+)x(\d+)$')) {
        $rect = @([int]$Matches[1], [int]$Matches[2], [int]$Matches[3], [int]$Matches[4])
      }
    }
    if ($null -eq $rect) { Ui-Fail "no $name card while Create Sketch waits for a plane: '$line'" }
    if ($ax -ge $rect[0] -and $ax -lt $rect[0] + $rect[2] -and $ay -ge $rect[1] -and $ay -lt $rect[1] + $rect[3]) {
      Ui-Fail "XY's place $at is inside the $name card ($($rect[0]),$($rect[1]) $($rect[2])x$($rect[3])): no place of XY to click in the view"
    }
  }
  Write-Host "ok   XY's place $at clear of the cards that take the mouse there ($($cards -join ', '))"
  Ui-ClickLogged 'Datum xy'
  Ui-ExpectNew 'Sketch started on xy' 'sketch started on xy from the click in the view' | Out-Null
}

# Sketch-OnXy: Create Sketch (Alt+S) on XY picked from the Browser's Origin
# folder, the other way to the same plane.
function Sketch-OnXy {
  Ui-Mark
  Ui-Key 'alt+s'
  Ui-ExpectNew 'Create Sketch: select a plane' 'Create Sketch asks for a plane' | Out-Null
  Ui-SyncOr 500
  Ui-PickOriginRow 'XY'
  Ui-ExpectNew 'Sketch started on xy' 'sketch started on xy' | Out-Null
}

# Click-PaletteFinish: the Sketch Palette's Finish Sketch, its last row. The
# palette scrolls in its place, and in session 0's 1024x768 window that row
# is below the place's edge (mitcad#696): wheel steps over the palette scroll
# it to its end, and it logs its places again when it scrolls.
function Click-PaletteFinish {
  Ui-WheelLogged 'Palette paletteHideAbove' -10
  # The scroll handled and its places logged (watched; mitcad#870: a fixed
  # 300 ms before the sync before).
  Ui-Sync
  Ui-ClickLogged 'Palette Finish Sketch'
}

Write-Host '--- 1. Without a choice Floating with Rows; the cards; Docked chosen, Later'
Ui-StartApp 'chrome-layout' @('--open', $plate) -DefaultChrome
Ui-ExpectLog "Chrome: floating (the platform's default)" 'Floating without a choice'
Ui-ExpectLog 'Browser background: rows' 'the Browser''s Rows without a choice (mitcad#2144)'
if ((Test-Path $ini) -and (Select-String -Path $ini -Pattern '^browserBackground=' -Quiet)) {
  Ui-Fail 'a Browser background saved without a choice'
}
Ui-ExpectLog 'Floating cards: window ' 'the cards placed'
# The default size is the session's (1028 wide in session 0 of the VM).
$entries = Cards-BelowBar 'at the default size' 1 100000
No-Gaps
Click-TitleRow $entries
# Without the plate for the rest (a sketch on XY).
Ui-StartApp 'chrome-layout-new' -DefaultChrome
Ui-ExpectLog "Chrome: floating (the platform's default)" 'Floating without a choice'
Choose-Layout 'Home'
Ui-ExpectNew 'Layout preference: docked (restart asked)' 'Docked chosen, a restart asked'
Ui-FocusDialog '^Restart MitCad$'
Ui-Step 'later (Escape)'                   { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Layout restart: later (docked at the next start)' 'kept for the next start'
if (-not (Select-String -Path $ini -Pattern '^layout=docked$' -Quiet)) { Ui-Fail 'the layout is not saved' }
Write-Host 'ok   saved as appearance/layout'

Write-Host '--- 2. Floating again: the window''s layout, no question'
Choose-Layout 'End'
Ui-ExpectNew 'Layout preference: floating (in use)' 'Floating is the window''s layout'
if (-not (Select-String -Path $ini -Pattern '^layout=floating$' -Quiet)) { Ui-Fail 'Floating is not saved' }
Write-Host 'ok   saved'

Write-Host '--- 3. Docked, Restart Now with unsaved changes: Cancel keeps Mitcad open'
Ui-Step 'create sketch, XY in the view'    { Sketch-OnXyInView }
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner at the origin'     { Ui-SketchClick 0 0 }
Ui-Step 'width 40 (Tab) height 25'       { Ui-Type '40'; Ui-Key 'Tab'; Ui-Type '25' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 40 x 25 mm at (0, 0)' 'a rectangle: a change to save'
Cards-BelowBar 'while sketching' 1 100000 | Out-Null
Ui-Mark
Ui-Step 'finish the sketch from ribbon'   { Ui-ClickLogged 'Ribbon button sketch.finish' }
Ui-ExpectLog 'Sketch finished' 'a change to save'
Ui-Step 'another sketch, XY in Browser'    { Sketch-OnXy }
Ui-ExpectNew 'Palette Finish Sketch at' 'palette Finish Sketch is laid out'
Cards-BelowBar 'in the next sketch' 1 100000 | Out-Null
Ui-Mark
Ui-Step 'finish the sketch from palette'  { Click-PaletteFinish }
Ui-ExpectNew 'Sketch finished' 'the palette still finishes a sketch'
Choose-Layout 'Home'
Ui-ExpectNew 'Layout preference: docked (restart asked)' 'a restart asked'
# The message boxes titled "MitCad" are new windows (Ui-FocusDialog).
$before = Ui-Windows '^MitCad$'
Ui-FocusDialog '^Restart MitCad$'
Ui-Step 'restart now (Enter)'              { Ui-Key 'Return' }
Ui-ExpectNew 'Layout restart: now' 'Restart Now'
Ui-ExpectNew 'Restart: closing the window' 'the window closes first'
Ui-FocusDialog '^MitCad$' -Except $before
Ui-Step 'cancel (Escape)'                  { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Restart cancelled: MitCad stayed open' 'cancelled as quitting would be'
if (Has-Line 'Restart (test, not started)') { Ui-Fail 'a restart after the cancel' }
Write-Host 'ok   Mitcad still runs, nothing restarted'

Write-Host '--- 4. Saved; Docked, Restart Now: Mitcad quits and starts again'
Ui-Mark
Ui-Step 'save as (Ctrl+Shift+S)'           { Ui-Key 'ctrl+shift+s' }
Ui-FocusDialog '^Save As$'
Ui-Step 'the path'                         { Ui-Key 'ctrl+a'; Ui-Type $file; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew "Saved $fileLogged" 'saved'
Choose-Layout 'End'
Ui-ExpectNew 'Layout preference: floating (in use)' 'Floating again'
Choose-Layout 'Home'
Ui-ExpectNew 'Layout preference: docked (restart asked)' 'a restart asked'
Ui-FocusDialog '^Restart MitCad$'
# No step: the process ends.
Ui-Key 'Return'
Ui-ExpectNew 'Restart (test, not started): ' "the restart's command" 20
$command = ' ' + (Ui-LastMatch '^Restart \(test, not started\): (.*)$') + ' '
if ($command.Contains(' --chrome')) { Ui-Fail "the restart keeps --chrome: $command" }
foreach ($part in @('--no-native-dialogs', '--no-recovery', "--open $fileLogged")) {
  if (-not $command.Contains(" $part ")) { Ui-Fail "the restart has no '$part': $command" }
}
Write-Host "ok   the run's settings options and the design, no --chrome:$command"
if (-not $script:UiProcess.WaitForExit(10000)) { Ui-Fail 'Mitcad did not exit to restart' }
# ExitCode is empty when the process handle was not read before it ended;
# the log tells a crash (Ui-Crashed and the crash folder are checked at the end).
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) { Ui-Fail "Mitcad exited with $exitCode" }
Write-Host 'ok   Mitcad quit to restart'

Write-Host '--- 5. The next start: Docked by the preference; Floating and the Card background chosen there'
Ui-StartApp 'chrome-layout-docked-pref' @('--open', $file) -DefaultChrome
Ui-ExpectLog 'Chrome: docked (the preference)' 'Docked by the preference'
if (Has-Line 'Floating cards: ') { Ui-Fail 'Floating cards in the Docked layout' }
Ui-Step 'create a Docked sketch'          { Ui-CreateSketch 'alt+s' 'xy' }
Ui-ExpectNew 'Palette Finish Sketch at' 'Docked palette finish is laid out'
Ui-Mark
Ui-Step 'finish Docked sketch from palette' { Click-PaletteFinish }
Ui-ExpectNew 'Sketch finished' 'Docked palette still finishes a sketch'
Ui-Step 'create another Docked sketch'    { Ui-CreateSketch 'alt+s' 'xy' }
Ui-Mark
Ui-Step 'finish Docked sketch from ribbon' { Ui-ClickLogged 'Ribbon button sketch.finish' }
Ui-ExpectNew 'Sketch finished' 'Docked ribbon still finishes a sketch'
Choose-Layout 'End' 'Home'
Ui-ExpectNew 'Preferences: browser background card' 'the Card background chosen while Docked'
Ui-ExpectNew 'Layout preference: floating (restart asked)' 'Floating chosen, a restart asked'
Ui-FocusDialog '^Restart MitCad$'
Ui-Step 'later (Escape)'                   { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Layout restart: later (floating at the next start)' 'kept for the next start'
if (-not (Select-String -Path $ini -Pattern '^layout=floating$' -Quiet)) { Ui-Fail 'the layout is not saved' }
if (-not (Select-String -Path $ini -Pattern '^browserBackground=card$' -Quiet)) {
  Ui-Fail 'the Browser background is not saved'
}
Write-Host 'ok   both saved for the next start'
Ui-StopApp

Write-Host '--- 6. The next start: Floating with the Card by the preferences; the cards'
Ui-StartApp 'chrome-layout-floating' @('--open', $plate) -DefaultChrome
Ui-ExpectLog 'Chrome: floating (the preference)' 'Floating by the preference'
Ui-ExpectLog 'Browser background: card' 'the Browser''s Card kept after a restart (mitcad#2144)'
Ui-ExpectLog 'Floating cards: window ' 'the cards placed'
$entries = Cards-BelowBar 'at the default size' 1 100000
Click-TitleRow $entries
# As narrow as the window gets: the ribbon's groups collapse.
Ui-Step 'a narrow window'                  { if (-not [LayoutTestWindow]::Resize($script:UiWindow, 800, 640)) { Ui-Fail 'the window was not resized' } }
Cards-BelowBar 'in a narrow window' 1 900 | Out-Null
Ui-Step 'a wide window'                    { if (-not [LayoutTestWindow]::Resize($script:UiWindow, 1500, 900)) { Ui-Fail 'the window was not resized' } }
# As wide as the desktop lets it (session 0's desktop may be narrower).
Cards-BelowBar 'in a wide window' 901 1500 | Out-Null
Ui-StopApp

Write-Host '--- 7. --chrome=docked overrides the preference'
Ui-StartApp 'chrome-layout-docked' @('--chrome=docked')
Ui-ExpectLog 'Chrome: docked (--chrome; the preference floating)' '--chrome decides, the preference named'
if (Has-Line 'Floating cards: ') { Ui-Fail 'Floating cards in the Docked layout' }
Write-Host 'ok   no cards'

Ui-Finish 'UI Windows chrome layout test'
