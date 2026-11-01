# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ChromeStyle.cpp app/framework/ChromeStyle.hpp app/framework/Restart.cpp
# check-all sources: app/framework/Restart.hpp app/framework/FloatingLayout.cpp app/framework/FloatingLayout.hpp
# check-all sources: app/framework/GlassCard.cpp app/framework/TitleBar.cpp app/framework/Ribbon.cpp
# check-all sources: app/main.cpp app/view/ViewController.cpp app/MainWindow.cpp app/OcctViewer.cpp tools/cli
# check-all sources: app/browser/BrowserBackground.cpp
# Input-driven Windows UI test of the window's layout chosen in Preferences
# (mitcad#419) and of the defaults (mitcad#429) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-chrome-layout-test.sh does on Linux:
#   1. Without a choice (a plate open) Floating with the Browser's Rows: only
#      the menu bar is above the view (mitcad#425); the title row (no card:
#      its controls and the title's box, each whole within it) and the group
#      cards float over its top, every other card starts below them; with
#      the plate zoomed in to fill the view, a click between two group cards
#      selects its face and a click on the title's box selects nothing.
#      Preferences > Display > Layout Docked asks Restart MitCad, Later
#      keeps it in the settings for the next start.
#   2. Floating again is the window's layout: saved, no question.
#   3. Docked, Restart Now with unsaved changes: the window asks about them
#      as quitting does; Cancel keeps Mitcad open and nothing restarts.
#   4. Saved, Docked, Restart Now: Mitcad quits and would start again
#      (MITCAD_TEST_NO_RESTART=1 logs the command) with the run's settings
#      options and the design, without --chrome.
#   5. The next start is Docked by the preference: no cards; Preferences
#      shows the Browser background there too (mitcad#429); Floating and
#      the Card background chosen together, Later.
#   6. The next start is Floating by the preference, with the Card
#      background: the cards at the default size, in a narrow and a wide
#      window; a click on the title row beside its controls reaches the
#      view.
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
# click between the group cards.
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
# window min to max wide (waited for): only the menu bar is above the view
# (the view's top is the bar's bottom, mitcad#425); the title row with its
# controls and the title's box, each whole within it (mitcad#429), and at
# least two group cards float over the view below the bar; the Browser's
# card and every other card start below the floating rows' bottom. Returns
# the line's entries by name: x, y, width, height.
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
  if ($bar -le 0) { Ui-Fail "${Description}: no top bar in '$line'" }
  if ($view -ne $bar) { Ui-Fail "${Description}: the view's top $view is not the menu bar's bottom $bar ($line)" }
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
    elseif ($name -ne 'ribbon' -and $name -ne 'noticeCard' -and -not $name.StartsWith('title') -and $y -lt $rows) {
      Ui-Fail "${Description}: ${name}'s top $y is above the floating rows' bottom $rows ($line)"
    }
    $count++
  }
  if (-not $entries.ContainsKey('titleRow')) { Ui-Fail "${Description}: no title row in '$line'" }
  if ($entries.ContainsKey('titleCard')) { Ui-Fail "${Description}: a title card in '$line'" }
  if ($entries.ContainsKey('titleFinishSketch')) { Ui-Fail "${Description}: duplicate Finish Sketch in the title row: '$line'" }
  foreach ($part in @('titleHistory', 'titleTabs', 'titleSearch')) {
    if (-not $entries.ContainsKey($part)) { Ui-Fail "${Description}: no $part in '$line'" }
  }
  # Every control of the row and the title's box whole within the row.
  $row = $entries['titleRow']
  foreach ($name in @($entries.Keys | Where-Object { $_.StartsWith('title') -and $_ -ne 'titleRow' })) {
    $part = $entries[$name]
    if ($part[0] -lt $row[0] -or $part[1] -lt $row[1] -or $part[0] + $part[2] -gt $row[0] + $row[2] -or
        $part[1] + $part[3] -gt $row[1] + $row[3]) {
      Ui-Fail "${Description}: $name ($($part -join ',')) is not within the title row ($($row -join ',')): '$line'"
    }
  }
  if ($groups -lt 2) { Ui-Fail "${Description}: $groups group cards in '$line'" }
  Write-Host "ok   ${Description}: the view below the menu bar ($bar), $count cards and parts, $groups group cards, the title row's parts within it, the others below the rows' bottom $rows (window $width wide)"
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

# Click-BetweenCards entries: with the plate filling the view, a click
# between two group cards selects its face, a click on the middle of the
# title's box selects nothing.
function Click-BetweenCards($Entries) {
  Zoom-Plate
  Ui-ExpectLog 'Floating view between the cards at' 'a place between two group cards is logged'
  Ui-Mark
  Ui-Step 'click between two group cards'  { Ui-ClickLogged 'Floating view between the cards' }
  Ui-ExpectNew 'Selected: 1 face [face F2:' "the click between the cards reached the view: the plate's face"
  if (-not $Entries.ContainsKey('titleBox')) { Ui-Fail 'no title box in the Floating cards line' }
  $box = $Entries['titleBox']
  $cx = $box[0] + [int]($box[2] / 2)
  $cy = $box[1] + [int]($box[3] / 2)
  Ui-Mark
  Ui-Step 'click the title box''s middle'  { Ui-Click $cx $cy }
  Start-Sleep -Milliseconds 500
  Ui-WaitIdle
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Selected: ') }) {
    Ui-Fail 'a click on the title box reached the view'
  }
  Write-Host 'ok   the title box takes its click'
}

# Click-TitleRow entries: with the plate filling the view and nothing
# selected, a click on the title row between the tabs and the next part
# (no card there, mitcad#429) selects the plate's face.
function Click-TitleRow($Entries) {
  Zoom-Plate
  $row = $Entries['titleRow']
  $tabs = $Entries['titleTabs']
  $next = $Entries.GetEnumerator() | Where-Object { $_.Key.StartsWith('title') -and $_.Key -ne 'titleRow' -and
    $_.Value[0] -gt $tabs[0] } | Sort-Object { $_.Value[0] } | Select-Object -First 1
  if (-not $next) { Ui-Fail 'nothing right of the title row''s tabs' }
  $cx = [int](($tabs[0] + $tabs[2] + $next.Value[0]) / 2)
  $cy = $row[1] + [int]($row[3] / 2)
  Ui-Mark
  Ui-Step "click the title row beside its controls ($cx,$cy)" { Ui-Click $cx $cy }
  Ui-ExpectNew 'Selected: 1 face [face F2:' "the click on the title row reached the view: the plate's face"
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
Ui-ExpectLog 'Browser background: rows' 'the Browser''s Rows without a choice'
Ui-ExpectLog 'Floating cards: window ' 'the cards placed'
# The default size is the session's (1028 wide in session 0 of the VM).
$entries = Cards-BelowBar 'at the default size' 1 100000
Click-BetweenCards $entries
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
Ui-Step 'create sketch on XY (Alt+S)'      { Ui-CreateSketch 'alt+s' 'xy' }
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner at the origin'     { Ui-SketchClick 0 0 }
Ui-Step 'width 40 (Tab) height 25'       { Ui-Type '40'; Ui-Key 'Tab'; Ui-Type '25' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 40 x 25 mm at (0, 0)' 'a rectangle: a change to save'
Cards-BelowBar 'while sketching' 1 100000 | Out-Null
Ui-Mark
Ui-Step 'finish the sketch from ribbon'   { Ui-ClickLogged 'Ribbon button sketch.finish' }
Ui-ExpectLog 'Sketch finished' 'a change to save'
Ui-Step 'create another sketch'           { Ui-CreateSketch 'alt+s' 'xy' }
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
Ui-ExpectLog 'Browser background: card' 'the Browser''s card by the preference'
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
