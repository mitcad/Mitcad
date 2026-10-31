# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowContextMenu.cpp app/browser app/commands/MoveCommands.cpp
# check-all sources: app/framework/Command.cpp app/framework/Command.hpp tools/cli
# Input-driven Windows UI test of Move/Copy in the context menus
# (mitcad#735) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), as tools/ui-move-copy-menu-test.sh does on Linux, on a design
# that mitcad-cli makes: a body of the root (Body1, F2.b0) and a component
# placed twice, Component1:1 (O1) and Component1:2 (O2), each with Body2
# (F3.b0). The context menu stays open in session 0 and takes the keys (as
# in tools/ui-windows-print-bed-test.ps1).
#   1. The browser: Move/Copy on Body1's row starts with Body1.
#   2. The browser: both occurrences selected, Move/Copy on a row of them
#      starts with both as components; Move to Component... is the
#      occurrence's structure move (no Move to...); OK moves them, one undo
#      takes it back.
#   3. The view: Body1 selected in the browser, a right click on its face
#      keeps it selected and offers Move/Copy at the menu's top; it starts
#      with Body1.
#   4. The view: a face alone has no Move/Copy.
#   5. In a design of its own, a right click on a face of a large body (a
#      filleted bracket with modelled M6 tapped holes and a modelled M6
#      threaded rod, in a placed occurrence) selects it, opens the menu and
#      logs the click's timing (mitcad#981: the selection card measured a
#      moved copy of the whole body, seconds on the UI thread). No time is
#      asserted.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-move-copy-menu-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-copy-menu'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.offset_face' = 'F7' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'design.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 50, 0]}}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# A design of its own (mitcad#981): a box 60 x 30 x 20 (F1) as
# Component1:1 (O1, F1.b0), its edges filleted (F2), three modelled M6
# tapped holes through it (F3), a 6 mm cylinder 60 high joined to it (F4)
# with a modelled M6 thread (F5); the occurrence placed 50 mm along -Y.
$bracketScript = Join-Path $work 'bracket.json'
$bracket = Join-Path $work 'bracket.mitcad'
Set-Content -Encoding ascii $bracketScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 60, "width": 30, "height": 20,
    "operation": "new_component"}},
  {"cmd": "activate_component", "component": "C1"},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F1.b0",
    "sets": [{"all_edges": true, "size": {"type": "constant", "radius": 2}}]}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F1.b0", "face": "F1:top", "points": [[10, 15, 20], [22, 15, 20], [34, 15, 20]]},
    "diameter": 5, "extent": {"type": "through_all"}, "thread": {"designation": "M6x1", "modeled": true}}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [50, 15], "diameter": 6, "height": 60,
    "operation": "join", "participants": ["F1.b0"]}},
  {"cmd": "add_feature", "def": {"type": "thread", "faces": [{"body": "F1.b0", "face": "F4:side0"}],
    "thread": {"designation": "M6x1"}, "modeled": true}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [0, -50, 0]}}
]
'@
$made = & $Cli run $bracketScript --save $bracket
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $bracket)) { Ui-Fail "mitcad-cli bracket: $($made -join ' / ')" }

# Menu-Entries: the entries of the context menu logged last (the labels'
# ellipsis as three dots).
$ellipsis = [string][char]0x2026
function Menu-Entries {
  return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ' | ForEach-Object { $_.Replace($ellipsis, '...') })
}
# Expect-Entry "entry" $true|$false: whether the context menu has it.
function Expect-Entry([string]$Entry, [bool]$Present) {
  $entries = Menu-Entries
  if (($entries -contains $Entry) -ne $Present) {
    Ui-Fail "'$Entry' $(if ($Present) { 'is not' } else { 'is' }) in the context menu: $($entries -join ' | ')"
  }
  Write-Host "ok   the context menu $(if ($Present) { 'has' } else { 'has no' }) $Entry"
}
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step $Entry {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}
# Expect-SameSelection: no "Selected:" line since the mark.
function Expect-SameSelection {
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.StartsWith('Selected: ') }) {
    Ui-Fail 'the right click changed the selection'
  }
  Write-Host 'ok   the right click kept the selection'
}

Ui-StartApp 'move-copy-menu' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Bodies/Body1 at' 'the browser lists Body1'
Ui-ExpectLog 'Browser Root/Component1:2 at' 'and Component1:2'
Ui-ExpectLog 'Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2' 'Body1 and both occurrences shown'

Write-Host '--- The browser: Move/Copy on a body''s row'
Ui-Mark
Ui-Step 'right-click Body1''s row'        { Ui-ClickLogged 'Browser Root/Bodies/Body1' 3 }
Ui-ExpectNew 'Context menu: ' 'the row''s menu'
Expect-Entry 'Move/Copy' $true
Menu-Choose 'Move/Copy'
Ui-ExpectNew 'Command Move/Copy started' 'Move/Copy started'
Ui-ExpectNew 'Move/Copy Objects: 1 body [body F2.b0]' 'with Body1'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Move/Copy cancelled' 'cancelled'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- The browser: Move/Copy on two occurrences'' rows'
Ui-Mark
Ui-Step 'click Component1:1'             { Ui-ClickLogged 'Browser Root/Component1:1' }
Ui-ExpectNew 'Selected: 1 component' 'Component1:1 selected'
$at = Ui-LoggedAt 'Browser Root/Component1:2'
Ui-Mark
Ui-Step 'Ctrl+click Component1:2'        { Ui-ClickWith 'ctrl' $at[0] $at[1] }
Ui-ExpectNew 'Selected: 2 components' 'both occurrences selected'
Ui-Mark
Ui-Step 'right-click Component1:2'       { Ui-ClickLogged 'Browser Root/Component1:2' 3 }
Ui-ExpectNew 'Context menu: ' 'the row''s menu'
Expect-SameSelection
Expect-Entry 'Move/Copy' $true
Expect-Entry 'Move to Component...' $true
Expect-Entry 'Move to...' $false
Menu-Choose 'Move/Copy'
Ui-ExpectNew 'Command Move/Copy started' 'Move/Copy started'
Ui-ExpectNew 'Panel Move/Copy choices: move_type=components' 'moving components'
Ui-ExpectNew 'Move/Copy Objects: 2 components [component ' 'with both occurrences'
Ui-ExpectNew 'Panel Move/Copy input tx at' 'the panel is laid out'
Ui-Step '20 mm along X'                  { Ui-TypeIn 'Panel Move/Copy input tx' '20' }
Ui-ExpectNew 'Preview Move/Copy: ok' 'previewed'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Moved 2 components (' 'both occurrences moved'
Ui-Mark
Ui-Step 'undo'                           { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'one undo takes the move back'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- The view: a body selected in the browser'
Ui-Mark
Ui-Step 'click Body1''s row'              { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Selected: 1 body [body F2.b0]' 'Body1 selected'
Ui-Mark
Ui-Step 'right-click Body1 in the view'  { Ui-ClickLogged 'Body F2.b0' 3 }
Ui-ExpectNew 'Context menu: ' 'the view''s menu'
Expect-SameSelection
Expect-Entry 'Move/Copy' $true
Menu-Choose 'Move/Copy'
Ui-ExpectNew 'Command Move/Copy started' 'Move/Copy started'
Ui-ExpectNew 'Move/Copy Objects: 1 body [body F2.b0]' 'with Body1'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Move/Copy cancelled' 'cancelled'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- The view: a face alone'
Ui-Mark
Ui-Step 'right-click Body1 in the view'  { Ui-ClickLogged 'Body F2.b0' 3 }
Ui-ExpectNew 'Selected: 1 face [face ' 'a face of Body1 selected'
Ui-ExpectNew 'Context menu: ' 'the view''s menu'
Expect-Entry 'Move/Copy' $false
Ui-Step 'Escape (close the menu)'        { Ui-Key 'Escape' }

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- The view: a face of a large body (modelled threads, placed)'
Ui-StartApp 'move-copy-menu-bracket' @('--open', $bracket) -ViewSeconds 120
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Body F1.b0 in O1 at' 'the bracket is in the view' 30
Ui-Mark
Ui-Step 'offset face (F7)'               { Ui-Key 'F7' }
Ui-ExpectNew 'Pick face F1.b0/F1:side0 at' 'where the bracket''s side is picked'
$at = Ui-PickAt 'face F1.b0/F1:side0 at'
Ui-Step 'cancel it (Esc)'                { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Offset Face cancelled' 'cancelled'
Ui-Mark
Ui-Step 'right-click the bracket''s side' { Ui-Click $at[0] $at[1] 3 }
Ui-ExpectNew 'Selected: 1 face [face F1:side0 of F1.b0 in O1]' 'the bracket''s side selected'
Ui-ExpectNew 'Selection timing: view ' 'the selection''s parts timed'
Ui-ExpectNew 'Context menu timing: select ' 'the right click timed'
Ui-ExpectNew 'Context menu: ' 'the view''s menu'
Write-Host "     $(Ui-LastMatch '^(Context menu timing: .*)$')"
Ui-Step 'Escape (close the menu)'        { Ui-Key 'Escape' }

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows Move/Copy menus'
