# SPDX-License-Identifier: MIT
# check-all sources: app/flex app/MainWindowFlexEdit.cpp app/MainWindow.cpp app/MainWindow.hpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindowNoticeHover.cpp
# check-all sources: app/commands/FlexCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: core/model-api/src/flex.rs core/model/src/features/flex.rs core/model/src/document/flex.rs
# check-all sources: geometry/src/flex.cpp geometry/src/flex_route.cpp geometry/src/flex_mesh.cpp tools/cli
# check-all sources: app/browser/FeaturePresentation.hpp app/browser/TimelineWidget.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/flex-edit-hover.mitcad-ui
# Input-driven Windows UI test of a Flex item edited in its edit session
# (mitcad#1713, app/COMMANDS.md "Flex editing", owner 14.11.2026) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-flex-edit-test.sh does on Linux, on a design that mitcad-cli
# makes: Box1 (F1, F1.b0) 40 x 30 x 20 at (20, 20), Flex1 (F2), a round
# hose of 6 mm from a free node n1 left of the box over n2 on the box's top
# to a free node n3 right of it, and Flex2 (F3), a short straight hose low
# right of the box, inside the others' bounds (the fit stays as it was),
# rolled back while Flex1's session runs.
#   1. A single click on Flex1's Timeline cell only selects it: no handles.
#   2. A double-click on its cell opens the edit session: the Flex panel on
#      the item and its handles beside it (nodes, both "+", insert points).
#   3. n3 dragged in the view: no route follows the pointer, the release
#      puts the change into the panel (no model command), and Undo in the
#      session takes it back.
#   4. A click on n2 takes the node in hand: the length is shown beside it;
#      Esc lets it go, the session stays.
#   5. A click on the route between n1 and n2 adds a node there, which is
#      the node in hand; Delete deletes it.
#   6. A click on the "+" at the end turns adding on: two clicks add two end
#      nodes; a click on the "+" again ends adding.
#   7. Enter keeps the session as one undo step; Undo takes all of it back.
#   8. A single click on the item in the view selects it without handles; a
#      double-click there opens the session again; Esc cancels it.
#   9. The app again, with the shared input script
#      tools/ui-scripts/flex-edit-hover.mitcad-ui (the in-app test driver's
#      pointer reaches the Timeline's tooltip, which a posted move does not
#      in session 0): with Flex1's session open, Flex2's hover text leaves
#      the session's types and end ports as it read them (mitcad#1971).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-flex-edit-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-flex-edit'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'hose.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [20, 20], "length": 40, "width": 30, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "flex",
    "item": {"name": "Hose 6", "section": {"type": "round", "diameter": 6}, "bend_radius": 10},
    "nodes": [
      {"id": "n1", "at": [-10, 35, 30]},
      {"id": "n2", "at": [40, 35, 20], "on": {"body": "F1.b0", "face": "F1:top"}},
      {"id": "n3", "at": [90, 35, 30]}]}},
  {"cmd": "add_feature", "def": {"type": "flex",
    "item": {"name": "Hose 6", "section": {"type": "round", "diameter": 6}, "bend_radius": 10},
    "nodes": [
      {"id": "n1", "at": [70, 45, 4]},
      {"id": "n2", "at": [88, 45, 4]}]}},
  {"expect": {"feature": "Flex1", "status": "ok"}},
  {"expect": {"feature": "Flex2", "status": "ok"}}
]
'@
$made = & $Cli run $modelScript --save $design 2>&1
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# The last "Flex edit ...: in the session; nodes ..." line's node ids.
function Session-Nodes {
  return "$(Ui-LastMatch 'Flex edit .*: in the session; nodes ([n0-9 ]+)$')".Trim()
}
# Fails when a line since the mark holds the text.
function Expect-NoNew([string]$Text, [string]$Failure) {
  if ((Ui-NewLines) | Where-Object { $_.Contains($Text) }) { Ui-Fail $Failure }
}
# A double-click at the last logged place, once more when nothing answered.
function DoubleClick-Expect([string]$Place, [string]$Line, [string]$Description) {
  for ($attempt = 1; $attempt -le 2; $attempt++) {
    Ui-Mark
    Ui-WaitIdle
    Ui-DoubleClickLogged $Place
    Ui-Sync
    for ($i = 0; $i -lt 15; $i++) {
      if ((Ui-NewLines) | Where-Object { $_.Contains($Line) }) { Write-Host "ok   $Description"; return }
      Start-Sleep -Milliseconds 200
    }
    Write-Host "note: no '$Line' after the double-click on '$Place' (attempt $attempt)"
    Start-Sleep -Milliseconds 450
  }
  Ui-Fail "${Description}: '$Line' not in the log after two double-clicks"
}

Ui-StartApp 'flex-edit' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }

Write-Host '--- 1. A single click selects the item, no handles'
Ui-Mark
Ui-Step 'click Flex1 in the timeline'    { Ui-ClickLogged 'Timeline Flex1' }
Ui-WaitIdle
Start-Sleep -Milliseconds 500
Expect-NoNew 'Flex edit shown' 'a single click showed the item''s handles'
Expect-NoNew 'Editing F2 with Flex' 'a single click opened the edit session'
Write-Host 'ok   selected, no handles, no session'
Start-Sleep -Milliseconds 600

Write-Host '--- 2. A double-click opens the edit session: the panel and the handles'
DoubleClick-Expect 'Timeline Flex1' 'Editing F2 with Flex' 'the double-click opens the Flex panel on the item'
Ui-ExpectNew 'Flex edit shown: F2 ' 'the handles are shown beside the panel'
Ui-ExpectNew 'Flex node n1 free at' 'the first node is a handle'
Ui-ExpectNew 'Flex node n2 face at' 'the node on the box''s top is a handle'
Ui-ExpectNew 'Flex node n3 free at' 'the last node is a handle'
Ui-ExpectNew 'Flex add start at' 'a + before the first node'
Ui-ExpectNew 'Flex add end at' 'a + after the last node'
Ui-ExpectNew 'Flex insert after n1 at' 'an insert point once the preview''s route is read'
Expect-NoNew 'Sketch started' 'editing a Flex item opened a sketch'

Write-Host '--- 3. A node dragged: into the panel, no model command; Undo in the session'
Ui-Mark
Ui-Step 'drag n3 up and to the right'    {
  $at = Ui-LoggedAt 'Flex node n3 free'
  Ui-Drag $at[0] $at[1] ($at[0] + 46) ($at[1] - 34)
}
Ui-ExpectNew 'Flex edit drag: started n3' 'the drag started on the node'
Ui-ExpectNew 'Flex edit target: ' 'the pointer gave the node a place'
Ui-ExpectNew 'Flex edit drag n3: in the session; nodes n1 n2 n3' 'the release changed the session'
Ui-ExpectNew 'Flex: Flex handles: drag n3 taken' 'the panel took the change'
Expect-NoNew 'Flex edit command: ' 'a drag in the session ran a model command'
Expect-NoNew 'Flex edit preview: ' 'a route followed the pointer in the session'
Expect-NoNew 'Flex route under the pointer: ' 'the panel''s route followed the pointer'
Write-Host 'ok   no model command, no route on the pointer'
Ui-ExpectNew 'Preview Flex: ok' 'the panel previewed the change' 60
Ui-WaitIdle
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Flex: undo in the edit: 0 change(s) left, 3 node(s)' 'Undo takes the session''s change back'
Ui-ExpectNew 'Flex: Undo taken' 'the panel took it'
Expect-NoNew 'Undo: ' 'Undo in the session undid a step of the model'
Ui-ExpectNew 'Preview Flex: ok' 'the panel previewed it' 60
Ui-WaitIdle

Write-Host '--- 4. A node in hand shows the length; Esc lets it go'
Ui-Mark
Ui-Step 'click n2'                       { Ui-ClickLogged 'Flex node n2 face' }
Ui-ExpectNew 'Flex node selected: n2' 'the node is in hand'
Ui-ExpectNew 'Flex edit badge: Length ' 'the length is shown beside it'
Ui-Mark
Ui-Step 'Esc'                            { Ui-Key 'Escape' }
Ui-ExpectNew 'Flex node selected: none' 'Esc lets the node go'
Expect-NoNew 'Command Flex cancelled' 'Esc with a node in hand cancelled the session'
Write-Host 'ok   the session stays'

Write-Host '--- 5. A node added on the route, and deleted'
Ui-Mark
Ui-Step 'click the route between n1 and n2' { Ui-ClickLogged 'Flex insert after n1' }
Ui-ExpectNew '"cmd":"flex.add_node"' 'a click on the route adds a node'
Ui-ExpectNew '"place":{"after":"n1"}' 'after n1'
Ui-ExpectNew 'Flex edit add after n1: in the session; nodes ' 'the session took it'
$nodes = (Session-Nodes) -split ' '
if ($nodes.Count -ne 4 -or $nodes[0] -ne 'n1' -or $nodes[2] -ne 'n2') { Ui-Fail "the nodes after the insert: $nodes" }
$added = $nodes[1]
Write-Host "ok   four nodes, $added between n1 and n2"
Ui-ExpectNew "Flex node selected: $added" 'the new node is in hand'
Ui-ExpectNew 'Preview Flex: ok' 'the panel previewed it' 60
Ui-WaitIdle
Ui-Mark
Ui-Step 'Delete'                         { Ui-Key 'Delete' }
Ui-ExpectNew '"cmd":"flex.delete_node"' 'Delete deletes the node in hand'
Ui-ExpectNew "Flex edit delete ${added}: in the session; nodes n1 n2 n3" 'three nodes again'
Ui-ExpectNew 'Preview Flex: ok' 'the panel previewed it' 60
Ui-WaitIdle

Write-Host '--- 6. + at the end: each click adds an end node until + is clicked again'
Ui-Mark
Ui-Step 'click + at the end'             { Ui-ClickLogged 'Flex add end' }
Ui-ExpectNew 'Flex edit add: started at the end' 'the + took the click'
Ui-ExpectNew 'Flex edit add: armed' 'adding nodes is on'
$plus = Ui-LoggedAt 'Flex add end'
Ui-Mark
Ui-Step 'click in the view'              { Ui-Click ($plus[0] + 40) ($plus[1] - 30) }
Ui-ExpectNew '"place":"end"' 'flex.add_node at the end'
Ui-ExpectNew 'Flex edit add at the end: in the session; nodes n1 n2 n3 ' 'the item is one node longer'
Ui-ExpectNew 'Flex edit add: armed at the end, each click adds a node' 'adding stays on'
Ui-ExpectNew 'Preview Flex: ok' 'the panel previewed it' 60
Ui-WaitIdle
Ui-Mark
Ui-Step 'click in the view again'        { Ui-Click ($plus[0] + 40) ($plus[1] - 80) }
Ui-ExpectNew 'Flex edit add at the end: in the session; nodes n1 n2 n3 ' 'and one more'
if (((Session-Nodes) -split ' ').Count -ne 5) { Ui-Fail "the nodes after two clicks: $(Session-Nodes)" }
Write-Host 'ok   five nodes, the old ends still nodes'
Ui-ExpectNew 'Preview Flex: ok' 'the panel previewed it' 60
Ui-WaitIdle
Ui-Mark
Ui-Step 'click + at the end again'       { Ui-ClickLogged 'Flex add end' }
Ui-ExpectNew 'Flex edit add: ended by a click on +' 'a click on the + ends adding'
Ui-Mark
$plus = Ui-LoggedAt 'Flex add end'
Ui-Step 'click in the view'              { Ui-Click ($plus[0] + 40) ($plus[1] + 60) }
Ui-WaitIdle
Expect-NoNew 'Flex edit add at the end' 'a click after the + was ended added a node'
Write-Host 'ok   the session as before: a click adds nothing'

Write-Host '--- 7. Enter keeps the session as one undo step'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F2' 'OK edits the item' 60
Ui-ExpectNew 'Flex edit hidden: F2' 'the handles go with the session'
Ui-WaitIdle
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Edit' 'one undo step takes the whole session back'
Ui-WaitIdle

Write-Host '--- 8. In the view: a click selects, a double-click opens the session, Esc cancels'
Ui-Mark
Ui-Step 'click the item in the view'     { Ui-ClickLogged 'Flex node n2 face' }
Ui-WaitIdle
Start-Sleep -Milliseconds 500
Expect-NoNew 'Flex edit shown' 'a single click in the view showed the handles'
Write-Host 'ok   selected, no handles'
Start-Sleep -Milliseconds 600
DoubleClick-Expect 'Flex node n2 face' 'Flex: double-click on F2 opens its edit session' 'a double-click on the item in the view opens its session'
Ui-ExpectNew 'Editing F2 with Flex' 'the Flex panel on the item' 30
Ui-ExpectNew 'Flex edit shown: F2 ' 'its handles beside it' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'Esc'                            { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Flex cancelled' 'Esc cancels the session'
Ui-ExpectNew 'Flex edit hidden: F2' 'the handles go with it'
if ((Ui-LogLines) | Where-Object { $_.Contains('OCCT view event failed') }) {
  Ui-Fail 'OCCT reported a failed view operation'
}

Write-Host '--- 9. Another Flex item''s hover text leaves the session''s types and end ports'
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/flex-edit-hover.mitcad-ui'
Ui-StartApp 'flex-edit-hover' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'the hover text driver timed out' }
$exitCode = $script:UiProcess.ExitCode
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the hover text driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Flex2''s hover text leaves the session''s types and end ports'

Ui-Finish 'UI Windows Flex editing test'
