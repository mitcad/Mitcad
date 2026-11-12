# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/PickContext.cpp app/PickContext.hpp app/MainWindow.cpp
# check-all sources: app/view app/browser app/sketch tools/cli
# Input-driven Windows UI test of the inactive components' ghosts and the
# active component first in picks (mitcad#420) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted); the Linux test
# is tools/ui-inactive-ghost-test.sh. A design of mitcad-cli: Part:1 (O1)
# with a 30 x 20 x 10 block, Shield:1 (O2) with a 40 x 2 x 20 plate in
# front of it, Cover:1 (O3) empty and active, saved so (Cover's sketches
# may lie on Part's faces):
#   1. Ghost Inactive Components off in the settings: opened with Cover
#      active, as saved (mitcad#1678): nothing is a ghost.
#   2. On (the default): opened with Cover active, the block and the
#      shield are ghosts.
#      Create Sketch picks the block's top face, Project picks it again
#      and projects it.
#   3. The root activated: nothing is a ghost; undo of the activation
#      brings the ghosts back, redo takes them away.
#   4. Part active, seen from the front: a click where the shield covers
#      the block takes the block's front face.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-inactive-ghost-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-inactive-ghost'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'view.front' = 'F8' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# Part:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1
# (F2.b0). Shield:1 (O2, C2): Box1 (F3, F3.b0) at y = -12 to -10. Cover:1
# (O3, C3), empty and active, beside them in the root.
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$lines = @('[',
  '  {"cmd": "create_component", "name": "Part"},',
  '  {"cmd": "sketch.create"},',
  '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},',
  ('  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
    '"}], "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},'),
  '  {"cmd": "activate_component", "component": "Root"},',
  '  {"cmd": "create_component", "name": "Shield"},',
  ('  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-5, -12], "length": 40, ' +
    '"width": 2, "height": 20, "operation": "new_body"}},'),
  '  {"cmd": "activate_component", "component": "Root"},',
  '  {"cmd": "create_component", "name": "Cover"},',
  '  {"cmd": "set_policy", "component": "Part", "policy": {"attachments_from_others": true, "components": false}},',
  '  {"cmd": "set_policy", "component": "Cover", "policy": {"attach_to_others": true}},',
  '  {"cmd": "set_policy", "component": "Root", "policy": {"attach_to_others": true}}',
  ']')
$script = Join-Path $work 'ghosts.json'
$design = Join-Path $work 'ghosts.mitcad'
Set-Content -Encoding ascii $script $lines
$output = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

Write-Host '--- 1. The setting off: no ghosts'
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
Add-Content -Encoding ascii $ini @('[view]', 'ghostInactive=false')
Ui-StartApp 'inactive-ghost-off' @('--open', $design)
Ui-ExpectLog 'F2.b0 in O1' "Part's block shown"
Ui-ExpectLog 'Opened with C3 active, as saved (unmodified)' 'opened with Cover active, as saved (mitcad#1678)'
Ui-Sync
Start-Sleep -Milliseconds 500
$ghosts = @(Ui-LogLines | Where-Object { $_.StartsWith('View inactive ghosts: ') -and -not $_.EndsWith(': none') })
if ($ghosts.Count -gt 0) { Ui-Fail "ghosts with the setting off: $($ghosts[0])" }
Write-Host 'ok   nothing is a ghost with the setting off'
Ui-StopApp

Write-Host '--- 2. The setting on: a ghost stays pickable'
(Get-Content $ini) -replace '^ghostInactive=false$', 'ghostInactive=true' | Set-Content -Encoding ascii $ini
Ui-StartApp 'inactive-ghost' @('--open', $design)
Ui-ExpectLog 'F2.b0 in O1' "Part's block shown"
Ui-ExpectLog 'Opened with C3 active, as saved (unmodified)' 'opened with Cover active, as saved (mitcad#1678)'
Ui-ExpectLog 'View inactive ghosts: 2 bodies (active Cover:1)' 'Cover active: the block and the shield are ghosts'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F2.b0/F2:end(' "the ghost's top face can be picked"
Ui-Step "pick the ghost's top face"      { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Sketch Sketch2 is in Cover' "the sketch is Cover's"
Ui-ExpectNew 'in O1, origin (0, 0, 10)' 'on the face where Part:1 shows it'
Ui-Mark
Ui-Step 'project (P)'                    { Ui-Key 'p' }
Ui-Step "pick the ghost's top face"      { Ui-SketchClick 15 10 }
Ui-ExpectNew 'Project Geometry: 1 face' "the ghost's face went to the projection"
Ui-ExpectNew 'Preview Project: ok' 'projection previewed'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Projected 1 item(s)' 'projected'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'

Write-Host '--- 3. Activation, undo and redo'
Ui-Mark
Ui-Step 'activate the root'              { Ui-ClickLogged 'Browser radio Root' }
Ui-ExpectNew 'Activated Root' 'the root is active'
Ui-ExpectNew 'View inactive ghosts: none' 'nothing is a ghost with the root active'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Activate Root' 'the activation undone'
Ui-ExpectNew 'View inactive ghosts: 2 bodies (active Cover:1)' 'Cover active again: the ghosts again'
Ui-Mark
Ui-Step 'redo (Ctrl+Y)'                  { Ui-Key 'ctrl+y' }
Ui-ExpectNew 'Redo: Activate Root' 'the activation redone'
Ui-ExpectNew 'View inactive ghosts: none' 'nothing is a ghost again'

Write-Host '--- 4. The active component first in picks'
Ui-Mark
Ui-Step 'activate Part'                  { Ui-ClickLogged 'Browser radio Root/Part:1' }
Ui-ExpectNew 'Activated Part' 'Part is active'
Ui-ExpectNew 'View inactive ghosts: 1 body (active Part:1)' "Part's block normal, the shield a ghost"
Ui-Mark
Ui-Step 'front view (F8)'                { Ui-Key 'F8' }
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectNew 'Camera direction 0 1 0' 'looking from the front: the shield covers the block'
Ui-ExpectNew 'Body F2.b0 in O1 at' "the block's place logged"
Ui-Mark
Ui-Step 'click where the shield covers the block' { Ui-ClickLogged 'Body F2.b0 in O1' }
Ui-ExpectNew 'Pick priority: active Part:1' "the active component's item taken first"
Ui-ExpectNew 'Selected: 1 face [face F2:side(' "the block's front face, behind the shield's ghost"

if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows inactive ghost test'
