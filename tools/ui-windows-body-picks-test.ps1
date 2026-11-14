# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model-api/src/sketch.rs
# Input-driven Windows UI test of body edges and vertices in the sketch
# plane (mitcad#256, mitcad#258) in the build VM (tools/ui-windows-lib.ps1
# says how the input is posted); the Linux test is
# tools/ui-sketch-body-picks-test.sh:
#   1. A design (mitcad-cli): a 60 x 40 block 20 high (its width k5, d1);
#      Sketch2 on its top face with free points at (20, 10) and (45, 25);
#      Sketch3 on the same face, empty.
#   2. Sketch Dimension in Sketch2: the block's front edge, the point at
#      (20, 10) and 15: the edge projected (linked) and the dimension added
#      in one undo step, which one undo takes back; again, and then from the
#      same edge to the other point: its projection used, none added.
#   3. Point tool in Sketch3: a click near a corner projects it and holds
#      the point on it; one near the middle of the front edge projects the
#      edge and holds the point at its middle.
#   4. The block 80 wide: both points follow (the saved design checked by
#      mitcad-cli); a click at the corner again adds nothing.
#   5. Sketch5 on the end face of a 24 cylinder beside the block, with a
#      hexagon, a line and a circle (mitcad#1782): Coincident between a
#      hexagon corner and the face's outer edge projects the edge (linked)
#      in the constraint's one undo step, which one undo takes back, and Esc
#      after the pick changes nothing; Tangent with the line projects it
#      again; Concentric with the circle reuses the projection.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-body-picks-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-body-picks'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Point tool has no shortcut of its own: the test gives it one, and the
# constraints function keys, as the other Windows sketch tests do.
Ui-Init $App $Out $QtBin @{
  'sketch.point' = 'F8'; 'sketch.constraint.coincident' = 'Shift+F5'; 'sketch.constraint.tangent' = 'Shift+F6'
  'sketch.constraint.concentric' = 'Shift+F7' } -StepPause 0

# Ui-NotSince "text" description: no line since the mark has the text.
function Ui-NotSince([string]$Text, [string]$Description) {
  Start-Sleep -Milliseconds 800
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains($Text) }) {
    Ui-Fail "${Description}: '$Text' logged"
  }
  Write-Host "ok   $Description"
}

# Ui-EditSketch name uid: a double-click on the sketch in the timeline, and
# the view fitted. (Ui-Step's own parameter is $Name: the action's
# variables are named otherwise.)
function Ui-EditSketch([string]$Sketch, [string]$Uid) {
  Ui-Mark
  Ui-Step "edit $Sketch (double-click)"  { Ui-DoubleClickLogged "Timeline $Sketch" }
  Ui-ExpectNew "Editing sketch $Uid ($Sketch)" "$Sketch edited" 30
  Ui-Step 'fit (F6)'                     { Ui-Key 'F6' }
  Start-Sleep -Milliseconds 800
}

# Ui-DimensionValue value: the value typed into the editor the placing
# click opened, and entered.
function Ui-DimensionValue([string]$Typed) {
  Ui-ExpectNew 'Sketch value editor at' 'the value editor opened' | Out-Null
  Ui-Step "type $Typed, Enter"           { Ui-Key 'ctrl+a'; Ui-Type $Typed; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
}

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'block.json'
$design = Join-Path $work 'block.mitcad'
Set-Content -Encoding ascii $script (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5", "text": [30, 6]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "F2:end(RECT)", "body": "F2.b0"}},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [20, 10]},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [45, 25]},
  {"cmd": "sketch.create", "plane": {"face": "F2:end(RECT)", "body": "F2.b0"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [-25, 20], "diameter": 24},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "F6:end(r{c1})", "body": "F6.b0"}},
  {"cmd": "sketch.polygon", "sketch": "F7", "center": [-25, 20], "vertex": [-19, 20], "sides": 6},
  {"cmd": "sketch.add_line", "sketch": "F7", "start": [-37, 36], "end": [-15, 37]},
  {"cmd": "sketch.circle", "sketch": "F7", "mode": "center", "center": [-50, 20], "radius": 3}
]
'@ -creplace 'RECT', 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}')
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'body-picks' @('--open', $design)
Ui-ExpectLog 'block.mitcad' 'the design opened'

Write-Host '--- Sketch Dimension from a block edge: one undo step (mitcad#256)'
Ui-EditSketch 'Sketch2' 'F3'
Ui-ExpectLog 'Sketch Sketch2: 4 DOF' 'two free points'
Ui-ExpectLog 'Sketch Sketch2: 0 linked projection(s)' 'nothing projected yet'
Ui-Mark
Ui-Step 'sketch dimension (D)'           { Ui-Key 'd' }
Ui-Step 'pick the front edge'            { Ui-SketchClick 30 0 }
Ui-ExpectNew 'Sketch plane geometry: 4 edge(s), 4 vertex(es)' 'the top face''s edges and corners are pickable'
Ui-ExpectNew 'Sketch Dimension picked the body edge E{' 'the edge picked'
Ui-Step 'pick the point (20, 10)'        { Ui-SketchClick 20 10 }
Ui-Step 'place the value'                { Ui-SketchClick 30 5 }
Ui-DimensionValue '15'
Ui-ExpectNew 'Projected the body edge E{' 'the edge projected when the dimension is made'
Ui-ExpectNew 'Added dimension k' 'a driving dimension'
Ui-ExpectNew 'Sketch Sketch2: 3 DOF' 'the point 15 mm from the edge'
Ui-ExpectNew 'Sketch Sketch2: 1 linked projection(s)' 'one linked projection'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Dimension to Sketch2' 'one undo step'
Ui-ExpectNew 'Sketch Sketch2: 4 DOF' 'the dimension gone'
Ui-ExpectNew 'Sketch Sketch2: 0 linked projection(s)' 'the projection gone with it'

Write-Host '--- Again, and from the same edge: its projection used'
Ui-Mark
Ui-Step 'sketch dimension (D)'           { Ui-Key 'd' }
Ui-Step 'pick the front edge'            { Ui-SketchClick 30 0 }
Ui-Step 'pick the point (20, 10)'        { Ui-SketchClick 20 10 }
Ui-Step 'place the value'                { Ui-SketchClick 30 5 }
Ui-DimensionValue '15'
Ui-ExpectNew 'Projected the body edge E{' 'the edge projected'
Ui-ExpectNew 'Sketch Sketch2: 3 DOF' 'the point 15 mm from the edge'
Ui-Mark
Ui-Step 'pick the front edge again'      { Ui-SketchClick 30 0 }
Ui-Step 'pick the point (45, 25)'        { Ui-SketchClick 45 25 }
Ui-Step 'place the value'                { Ui-SketchClick 52 12 }
Ui-DimensionValue '30'
Ui-ExpectNew 'Added dimension k' 'a second dimension'
Ui-ExpectNew 'Sketch Sketch2: 2 DOF' 'the second point 30 mm from the edge'
Ui-NotSince 'Projected the body' 'the edge not projected a second time'
Ui-NotSince 'Sketch Sketch2: 2 linked projection(s)' 'still one linked projection'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch2 finished'

Write-Host '--- Point tool on a corner and at an edge''s middle (mitcad#258)'
Ui-EditSketch 'Sketch3' 'F4'
Ui-ExpectLog 'Sketch Sketch3: 0 linked projection(s)' 'nothing projected yet'
Ui-Mark
Ui-Step 'point tool (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'Sketch tool Point' 'the Point tool'
Ui-Mark
Ui-Step 'click near the corner (60, 40)' { Ui-SketchClick 59.8 39.7 }
Ui-ExpectNew 'Projected the body vertex V{' 'the corner projected'
Ui-ExpectNew 'Added point p' 'a point'
Ui-ExpectNew 'coincident with p' 'on the projected corner'
Ui-ExpectNew 'Sketch Sketch3: 1 linked projection(s)' 'one linked projection'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Point to Sketch3' 'one undo step'
Ui-ExpectNew 'Sketch Sketch3: 0 linked projection(s)' 'the projection gone with the point'
Ui-Mark
Ui-Step 'click near the corner again'    { Ui-SketchClick 59.8 39.7 }
Ui-ExpectNew 'Projected the body vertex V{' 'the corner projected'
Ui-ExpectNew 'coincident with p' 'the point on it'
Ui-Mark
Ui-Step 'click near the front edge''s middle' { Ui-SketchClick 30.3 0.2 }
Ui-ExpectNew 'Projected the body edge E{' 'the edge projected'
Ui-ExpectNew 'at the midpoint of c' 'the point at its middle'
Ui-ExpectNew 'Sketch Sketch3: 2 linked projection(s)' 'two linked projections'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch3 finished'

Write-Host '--- The block 80 wide'
Ui-EditSketch 'Sketch1' 'F1'
Ui-ExpectNew 'Dimension k5 text at' 'the width''s value placed'
Ui-Mark
Ui-Step 'double-click the width'         { Ui-DoubleClickLogged 'Dimension k5 text' }
Ui-ExpectNew 'Editing dimension k5' 'the width''s value editor'
Ui-Step 'type 80, Enter'                 { Ui-Key 'ctrl+a'; Ui-Type '80'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew 'Dimension k5 = 80 mm (d1)' 'the width changed'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch1 finished'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- The same corner again: nothing added'
Ui-EditSketch 'Sketch3' 'F4'
Ui-ExpectLog 'Sketch Sketch3: 2 linked projection(s)' 'the two projections'
Ui-Step 'point tool (F8)'                { Ui-Key 'F8' }
Ui-Mark
Ui-Step 'click the corner (80, 40)'      { Ui-SketchClick 79.8 39.7 }
Ui-NotSince 'Projected the body' 'the corner not projected again'
Ui-NotSince 'Added point' 'no second point'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch3 finished'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'

Write-Host '--- The points follow the block'
# The design as the model evaluates it: Sketch3's projected corner and edge
# where the block is now, the points held on them; Sketch2 has one edge
# projection and two driving dimensions.
$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @'
[
  {"expect": {"query": {"query": "sketch", "uid": "F4"}, "result": {
    "entities": [{"type": "point", "at": [80, 40]}, {"type": "point", "at": [80, 40]},
                 {"type": "point"}, {"type": "point"}, {"type": "line"},
                 {"type": "point", "at": [40, 0]}],
    "constraints": [{"type": "coincident"}, {"type": "midpoint"}],
    "projections": [{"body": "F2.b0", "by_geometry": true}, {"body": "F2.b0", "by_geometry": true}],
    "warnings": []}}},
  {"expect": {"query": {"query": "sketch", "uid": "F3"}, "result": {
    "projections": [{"body": "F2.b0", "by_geometry": true}],
    "dimensions": [{"type": "point_line_distance", "measured": 15},
                   {"type": "point_line_distance", "measured": 30}]}}}
]
'@
$checked = & $cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved design: $($checked -join ' / ')" }
Write-Host 'ok   both points follow the block: the corner (80, 40), the edge''s middle (40, 0)'

Write-Host '--- Sketch5 on the cylinder''s end face: constraints to its edge (mitcad#1782)'
# The face's outer edge, a circle of 12 about (-25, 20), is picked at its
# lower left (-33.49, 11.51), away from the hexagon (its corners 6 from the
# centre).
Ui-EditSketch 'Sketch5' 'F7'
Ui-ExpectLog 'Sketch Sketch5: 11 DOF' 'a hexagon, a line and a circle, free'
Ui-ExpectLog 'Sketch Sketch5: 0 linked projection(s)' 'nothing projected yet'
Ui-Mark
Ui-Step 'coincident (Shift+F5)'          { Ui-Key 'shift+F5' }
Ui-ExpectNew 'Sketch tool Coincident' 'Coincident active'
Ui-Step 'pick the face''s edge'          { Ui-SketchClick -33.49 11.51 }
Ui-ExpectNew 'Coincident picked the body edge E{' 'the edge in the sketch plane picked'
Ui-Step 'Esc'                            { Ui-Key 'Escape' }
Ui-NotSince 'Projected the body' 'Esc after the pick: nothing projected'
Ui-NotSince 'Added constraint' 'no constraint added'

Write-Host '--- Coincident: a hexagon''s corner on the edge, in one undo step'
Ui-Mark
Ui-Step 'pick the corner (-19, 20)'      { Ui-SketchClick -19 20 }
Ui-Step 'pick the face''s edge'          { Ui-SketchClick -33.49 11.51 }
Ui-ExpectNew 'Projected the body edge E{' 'the edge projected when the constraint is added'
Ui-ExpectNew 'Added constraint coincident k' 'the corner on the projected edge'
Ui-ExpectNew 'Sketch Sketch5: 10 DOF' 'the corner held on the circle'
Ui-ExpectNew 'Sketch Sketch5: 1 linked projection(s)' 'one linked projection'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Constraint to Sketch5' 'one undo step, named after the constraint'
Ui-ExpectNew 'Sketch Sketch5: 11 DOF' 'the constraint gone'
Ui-ExpectNew 'Sketch Sketch5: 0 linked projection(s)' 'the projection gone with it'

Write-Host '--- Tangent: the line to the edge'
Ui-Mark
Ui-Step 'tangent (Shift+F6)'             { Ui-Key 'shift+F6' }
Ui-ExpectNew 'Sketch tool Tangent' 'Tangent active'
Ui-Step 'pick the line'                  { Ui-SketchClick -26 36.5 }
Ui-Step 'pick the face''s edge'          { Ui-SketchClick -33.49 11.51 }
Ui-ExpectNew 'Projected the body edge E{' 'the edge projected'
Ui-ExpectNew 'Added constraint tangent k' 'the line tangent to the projected edge'
Ui-ExpectNew 'Sketch Sketch5: 10 DOF' 'the line held'
Ui-ExpectNew 'Sketch Sketch5: 1 linked projection(s)' 'one linked projection'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }

Write-Host '--- Concentric: the circle with the edge, its projection reused'
Ui-Mark
Ui-Step 'concentric (Shift+F7)'          { Ui-Key 'shift+F7' }
Ui-ExpectNew 'Sketch tool Concentric' 'Concentric active'
Ui-Step 'pick the circle'                { Ui-SketchClick -47 20 }
Ui-Step 'pick the face''s edge'          { Ui-SketchClick -33.49 11.51 }
Ui-ExpectNew 'Added constraint concentric k' 'the circle concentric with the projected edge'
Ui-ExpectNew 'Sketch Sketch5: 8 DOF' 'the circle''s centre held at the face''s'
Ui-NotSince 'Projected the body' 'the edge not projected a second time'
Ui-NotSince 'Sketch Sketch5: 2 linked projection(s)' 'still one linked projection'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch5 finished'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows body picks test'
