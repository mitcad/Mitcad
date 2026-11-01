# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandRegistry.cpp app/framework/CommandRegistry.hpp
# check-all sources: app/framework/ShortcutBindings.hpp app/framework/ShortcutDialog.cpp app/MainWindow.cpp
# Input-driven Windows UI test of keys per context (mitcad#720) in the build
# VM (tools/ui-windows-lib.ps1 says how the input is posted), as the keys
# per context steps of tools/ui-command-test.sh on Linux:
#   1. Keyboard Shortcuts (F9 in the test's settings), by its keys: Command
#      Search's row expanded, its Sketch row given F3, Fit Point Spline (a
#      sketch command) given S; OK saves them, the app logs Command Search's
#      keys per context (S in 3D View and Command Panel, F3 in Sketch) and no
#      conflict;
#   2. F3 for the spline too: OK names the conflict and its context and
#      selects the spline's row; the window stays open; filtered to Sketch,
#      Command Search is one row of its Sketch key, and F5 there leaves its
#      3D View and Command Panel keys alone; with one context filtered the
#      Context column is hidden and the selected row is kept over the filter
#      changes (mitcad#730); the reserved Ctrl+F is a read-only row under
#      Search Timeline; the window cancels;
#   3. S opens the command search in the model;
#   4. in a sketch (Create Sketch by its default Alt+S), S starts the
#      spline and opens no search, F3 opens the search.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-shortcut-contexts-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-shortcut-contexts'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Keyboard Shortcuts by a key: the command search's popup closes at once in
# session 0.
Ui-Init $App $Out $QtBin @{ 'tools.shortcuts' = 'F9' } -StepPause 0

# Searches-Since-Mark: the command searches opened since Ui-Mark.
function Searches-Since-Mark {
  return @(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Command search opened') }).Count
}

Ui-StartApp 'shortcut-contexts'
if ((Ui-LogLines) | Where-Object { $_.Contains('Conflicting saved shortcuts') }) { Ui-Fail 'the default shortcuts conflict' }
Write-Host 'ok   no default shortcut in conflict'

Write-Host '--- 1. Keyboard Shortcuts: Command Search F3 in Sketch, Fit Point Spline S'
Ui-Mark
Ui-Step 'Keyboard Shortcuts (F9)'         { Ui-Key 'F9' }
Ui-ExpectNew 'Keyboard Shortcuts opened' 'the window opened'
Ui-FocusDialog '^Keyboard Shortcuts$'
Ui-Step 'find Command Search'             { Ui-Type 'Command Search' }
Ui-Step 'to its row (Down)'               { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts row: Command Search | All | S' 'one row, S in all its contexts'
Ui-Step 'expand it (Right)'               { Ui-Key 'Right' }
Ui-ExpectNew 'Keyboard Shortcuts expanded: Command Search' 'its contexts shown'
Ui-Step 'its Sketch row (Down Down)'      { Ui-Key 'Down', 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts row: Command Search | Sketch | S' 'its Sketch row'
Ui-Step 'record its key (Enter)'          { Ui-Key 'Return' }
Ui-ExpectNew 'Keyboard Shortcuts recording: Command Search | Sketch' 'recording the key'
Ui-Step 'F3'                              { Ui-Key 'F3' }
Ui-ExpectNew 'Keyboard Shortcuts key: Command Search | Sketch = F3' 'F3 in Sketch'
Ui-ExpectNew 'Keyboard Shortcuts row: Command Search | All | Varies' 'its keys vary by context'
Ui-Step 'back to the search (Shift+Tab)'  { Ui-Key 'shift+Tab', 'shift+Tab' }
Ui-Step 'find Fit Point Spline'           { Ui-Type 'Fit Point Spline' }
Ui-Step 'to its row (Down)'               { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts row: Fit Point Spline | Sketch | (none)' "the spline's row"
Ui-Step 'record its key (Enter)'          { Ui-Key 'Return' }
Ui-Step 'S'                               { Ui-Key 's' }
Ui-ExpectNew 'Keyboard Shortcuts key: Fit Point Spline | Sketch = S' 'S for the spline'
Ui-Step 'OK (Enter in the search)'        { Ui-Key 'shift+Tab', 'shift+Tab', 'Return' }
Ui-ExpectNew 'Keyboard Shortcuts saved' 'OK passed'
Ui-ExpectNew 'Shortcut keys of tools.search by context: 3D View S, Sketch F3, Command Panel S' `
  "Command Search's keys per context"
if ((Ui-LogLines) | Where-Object { $_.Contains('Conflicting saved shortcuts') }) { Ui-Fail 'the saved shortcuts conflict' }
Write-Host 'ok   no shortcut in conflict'
Ui-FocusMain

Write-Host '--- 2. Keyboard Shortcuts: OK refuses F3 for the spline too'
Ui-Mark
Ui-Step 'Keyboard Shortcuts (F9)'         { Ui-Key 'F9' }
Ui-ExpectNew 'Keyboard Shortcuts opened' 'the window opened'
Ui-FocusDialog '^Keyboard Shortcuts$'
$windows = Ui-Windows '^Keyboard Shortcuts$'
Ui-Step 'find Fit Point Spline'           { Ui-Type 'Fit Point Spline' }
Ui-Step 'to its row (Down)'               { Ui-Key 'Down' }
Ui-Step 'record its key (Enter)'          { Ui-Key 'Return' }
Ui-Step 'F3'                              { Ui-Key 'F3' }
Ui-ExpectNew 'Keyboard Shortcuts key: Fit Point Spline | Sketch = F3' 'F3 for the spline'
Ui-Step 'OK (Enter in the search)'        { Ui-Key 'shift+Tab', 'shift+Tab', 'Return' }
Ui-ExpectNew ' in a sketch. Change one of them.' 'OK names the conflict and its context'
Ui-ExpectNew 'Keyboard Shortcuts conflict at: Fit Point Spline | Sketch' 'the conflicting row selected'
if (@(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Keyboard Shortcuts saved') }).Count -ne 0) {
  Ui-Fail 'a conflict was saved'
}
Ui-FocusDialog '^Keyboard Shortcuts$' -Except $windows
Ui-Step 'close the message (Enter)'       { Ui-Key 'Return' }
Ui-FocusDialog '^Keyboard Shortcuts$'
Write-Host '--- Keyboard Shortcuts filtered to Sketch: a key there changes that context alone'
Ui-Mark
Ui-Step 'to the context list (Shift+Tab)' { Ui-Key 'shift+Tab' }
Ui-Step '3D View (Down)'                    { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts context filter: 3D View' 'the 3D View context filtered on the way'
# The Sketch filter's own lines from here on.
Ui-Mark
Ui-Step 'Sketch (Down)'                   { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts context filter: Sketch' 'the Sketch context filtered'
# mitcad#730: no Context column repeating Sketch; the spline's row, not
# shown in 3D View on the way, selected again.
Ui-ExpectNew 'Keyboard Shortcuts columns: Command, Shortcut' 'the Context column hidden'
Ui-ExpectNew 'Keyboard Shortcuts selection kept: Fit Point Spline | Sketch' "the spline's row still selected"
Ui-Step 'back to the search (Shift+Tab)'  { Ui-Key 'shift+Tab' }
Ui-Step 'find Command Search'             { Ui-Type 'Command Search' }
Ui-Step 'to its row (Down)'               { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts row: Command Search | Sketch | F3' 'one row: its Sketch key'
Ui-Step 'record its key (Enter)'          { Ui-Key 'Return' }
Ui-Step 'F5'                              { Ui-Key 'F5' }
Ui-ExpectNew 'Keyboard Shortcuts key: Command Search | Sketch = F5' 'F5 in Sketch'
Ui-ExpectNew 'Keyboard Shortcuts keys of Command Search: 3D View S, Sketch F5, Command Panel S' `
  '3D View and Command Panel keep S'
Write-Host '--- Keyboard Shortcuts: all contexts again, the selection kept (mitcad#730)'
Ui-Mark
Ui-Step 'to the context list (Shift+Tab)' { Ui-Key 'shift+Tab' }
Ui-Step 'All Contexts (Home)'             { Ui-Key 'Home' }
Ui-ExpectNew 'Keyboard Shortcuts context filter: All Contexts' 'all contexts shown'
Ui-ExpectNew 'Keyboard Shortcuts columns: Command, Context, Shortcut' 'the Context column back'
Ui-ExpectNew 'Keyboard Shortcuts selection kept: Command Search | Sketch' "Command Search's Sketch row selected"
# The row the user chose stays the one kept: 3D View on the way selects the
# 3D View row, and All Contexts again the Sketch row.
Ui-Mark
Ui-Step '3D View (Down)'                    { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts selection kept: Command Search | 3D View' 'its 3D View row in 3D View'
Ui-Mark
Ui-Step 'All Contexts (Up)'               { Ui-Key 'Up' }
Ui-ExpectNew 'Keyboard Shortcuts context filter: All Contexts' 'all contexts again'
Ui-ExpectNew 'Keyboard Shortcuts selection kept: Command Search | Sketch' 'the Sketch row chosen first, again'
Write-Host '--- Keyboard Shortcuts: the reserved Ctrl+F under Search Timeline, read-only (mitcad#730)'
Ui-Mark
Ui-Step 'back to the search (Shift+Tab)'  { Ui-Key 'shift+Tab' }
Ui-Step 'find Search Timeline'            { Ui-Type 'Search Timeline' }
Ui-Step 'to its row (Down)'               { Ui-Key 'Down' }
Ui-ExpectNew 'Keyboard Shortcuts row: Search Timeline | All | (none)' 'the command, no key of its own'
Ui-Step 'to its last row (End)'           { Ui-Key 'End' }
Ui-ExpectNew 'Keyboard Shortcuts row: Search Timeline | All | Ctrl+F (reserved)' 'its reserved Ctrl+F'
Ui-Step 'try to record a key (Enter)'     { Ui-Key 'Return' }
Ui-ExpectNew 'Keyboard Shortcuts reserved: Search Timeline | Ctrl+F is reserved for Search Timeline.' `
  'a reserved key cannot be changed, and says so'
if (@(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Keyboard Shortcuts recording:') }).Count -ne 0) {
  Ui-Fail 'a reserved key was recorded'
}
Write-Host 'ok   no recording on the reserved row'
# Under the command, not a row of its own: Left goes up to the command.
Ui-Mark
Ui-Step 'to its parent (Left)'            { Ui-Key 'Left' }
Ui-ExpectNew 'Keyboard Shortcuts row: Search Timeline | All | (none)' 'Ctrl+F is a row under Search Timeline'
Ui-Step 'cancel (Esc)'                    { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 3. S in the model: Command Search'
Ui-Mark
Ui-Step 'search (S)'                      { Ui-Key 's' }
Ui-ExpectNew 'Command search opened' 'S opens the command search in the model'
# The search's popup closes at once in session 0; Esc for a popup left open.
Ui-FocusMain
Ui-Step 'close the search (Esc)'          { Ui-Key 'Escape' }
Ui-ExpectNew 'Command search closed' 'the search closed'
Ui-FocusMain

Write-Host '--- 4. S and F3 in a sketch'
Ui-Step 'create sketch on XY (Alt+S)'     { Ui-CreateSketch 'alt+s' 'xy' }
Ui-Mark
Ui-Step 'spline (S)'                      { Ui-Key 's' }
Ui-ExpectNew 'Sketch tool Fit Point Spline' 'S starts the spline in a sketch'
if ((Searches-Since-Mark) -ne 0) { Ui-Fail 'S opened the command search in a sketch' }
Write-Host 'ok   S opens no command search in a sketch'
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'search (F3)'                     { Ui-Key 'F3' }
Ui-ExpectNew 'Command search opened' 'F3 opens the command search in a sketch'
Ui-FocusMain
Ui-Step 'close the search (Esc)'          { Ui-Key 'Escape' }
Ui-ExpectNew 'Command search closed' 'the search closed'
Ui-FocusMain
Ui-Mark
Ui-Step 'finish the sketch from ribbon'   { Ui-ClickLogged 'Ribbon button sketch.finish' }
Ui-ExpectNew 'Sketch finished' 'the sketch finished'

Ui-Finish 'UI Windows shortcut contexts test'
