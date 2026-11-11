# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/MainWindowSketch3d.cpp app/MainWindow.cpp app/MainWindow.hpp
# check-all sources: app/MainWindowVisibility.cpp app/MainWindowSelection.cpp app/framework/ModelShapes.cpp
# check-all sources: core/model-api/src/sketch3d.rs core/model/src/document/browser.rs tools/cli
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: core/sketch/src/spatial_convert.rs core/model/src/document/sketch_conversion.rs
# check-all sources: core/sketch/src/spatial_edit.rs
# check-all sources: app/commands core/model/src/features/spatial_path.rs core/model/src/features/path.rs
# check-all sources: core/model/src/features/rib.rs
# Input-driven Windows UI test of 3D sketches in the shared Qt UI
# (mitcad#1524) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), as tools/ui-sketch3d-test.sh does on Linux, on a design
# that mitcad-cli makes: Box1 (F1, F1.b0) 40 x 30 x 20 at (20, 20), off the
# origin planes (a face on one would take the plane's pick); 3DSketch1 (F2) on
# the box's top face with a line from the origin along X, 40 long, a
# tangent arc, a fit-point spline, a reference curve of the box's upright
# edge and a closed triangle of lines; Sketch1 (F3), planar, after it.
#   1. The Browser lists 3DSketch1 under Sketches with its freedoms.
#   2. Create Sketch (Alt+S) shows Sketch type 2D and 3D before the plane;
#      3D chosen and XZ picked make an empty 3D sketch, the banner says 3D
#      Sketch, and Finish takes the empty new sketch back.
#   3. Edit 3DSketch1 from the Timeline: its frame 20 up on the face, the
#      2D tools and Extrude disabled with the reason, p2 picked by its id
#      and described in the 3D Sketch Palette, Esc clears the selection and
#      stays in the sketch, Finish.
#   4. The drawing tools (mitcad#1525) in a new 3D sketch on XZ: Line (L),
#      a click on the origin snaps and is pending, Delta X 40 and an
#      explicit Delta Z 0 typed, Enter commits AB with Delta Y free; Esc
#      takes back the field, then the chain, then the tool; a Line from
#      the origin whose end snaps onto a side face of the box with the On
#      Face badge (mitcad#1549), the relation and the face's record in the
#      palette's lists; Finish.
#   5. Editing (mitcad#1526): 3DSketch1 again, AB's length typed 50 in the
#      palette's list (W2) and removed (W3), B dragged along X as one undo
#      step, undone; Finish.
#   6. Converting (mitcad#1544): Sketch1 edited, the Sketch Palette's 3D
#      Sketch box turns it into a 3D sketch in place, fully constrained
#      with its relations in plane, the 3D Sketch Palette's box checked
#      and disabled with why; Undo is one step back to the 2D editor;
#      Finish.
#   7. Fillet (mitcad#1548, F7 the test's key): 3DSketch1 again, Radius 3
#      typed, a click on the triangle's corner p12 rounds it at once as one
#      sketch3d.fillet; Esc ends the tool (after the field's value when
#      the field kept the keyboard); Finish as one step, undone.
#   8. Rib from 3DSketch1's curves (mitcad#1550, Shift+F8 the test's key),
#      before the pipes run round them: a click on the spline that leaves
#      the face is refused, saying it does not lie in the sketch plane; a
#      click on the line AB in the plane is taken; Esc.
#   9. Pipe along 3D sketches (mitcad#1527, F9 the test's key): one pick of
#      the route's arc takes the whole route with Chain Selection (not the
#      triangle, a closed chain of its own), listed from A, Reverse
#      offered, OK; Pipe1 edited shows its route as picked; Esc.
#  10. A closed path (mitcad#1550): one pick of a line of the triangle takes
#      it whole, listed closed from p11, Pipe2 round it.
#  11. Plane-bound tools (mitcad#1550) in a new 3D sketch on XZ: the SKETCH
#      tab's 2-Point Rectangle (R) from the origin with Width 30 and Height
#      20 typed is one sketch3d.add_rectangle with the origin's attachment,
#      fully constrained, one undo step (undone, redone); Center Diameter
#      Circle (C) on the YZ plane around that corner with Radius 8; Finish.
#  12. Plane-bound tools in another 3D sketch on XZ (keys of the test's own):
#      Ellipse (F3) from the origin with Major Radius 10 and Minor Radius 5
#      typed, one sketch3d.add_ellipse; Circumscribed Polygon (F5) around
#      the origin with Radius 20 and its 6 sides, one sketch3d.add_polygon;
#      Text (F11) in a frame dragged left of the origin, its preview logged
#      while it is dragged, "MitCad" typed, one sketch3d.add_text with the
#      frame; Finish.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch3d-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch3d'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Keys of the test's own for the tools without one (Pipe, Rib: the command
# search's popup closes at once in session 0).
Ui-Init $App $Out $QtBin @{ 'sketch.arc_center' = 'F8'; 'sketch.fillet' = 'F7'; 'solid.pipe' = 'F9';
                            'sketch.ellipse' = 'F3'; 'sketch.polygon' = 'F5'; 'sketch.text' = 'F11';
                            'solid.rib' = 'Shift+F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'route.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [20, 20], "length": 40, "width": 30, "height": 20, "operation": "new_body"}},
  {"cmd": "sketch3d.create", "support": {"body": "F1.b0", "face": "F1:top"}},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": [0, 0, 0], "end": [38, 1, 0], "axis": "x",
    "length": 40, "start_source": "origin"},
  {"cmd": "sketch3d.add_arc", "sketch": "F2", "mode": "tangent", "from": "c3", "start": "p2",
    "end": [50, 10, 0], "radius": 10},
  {"cmd": "sketch3d.add_spline", "sketch": "F2", "points": ["p5", [50, 30, 10], [50, 40, 25]]},
  {"cmd": "sketch3d.include", "sketch": "F2", "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"}},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": [0, -20, 0], "end": [20, -20, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": "p12", "end": [10, -35, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": "p14", "end": "p11"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [60, 0], "width": 10, "height": 10},
  {"expect": {"feature": "3DSketch1", "status": "ok"}}
]
'@
$made = & $Cli run $modelScript --save $design 2>&1
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Ui-NotSince "text" "description": no line since the mark has the text.
function Ui-NotSince([string]$Text, [string]$Description) {
  Ui-Sync
  Start-Sleep -Milliseconds 500
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains($Text) }) {
    Ui-Fail "${Description}: '$Text' is in the log"
  }
  Write-Host "ok   $Description"
}

Ui-StartApp 'sketch3d' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Sketches/3DSketch1 at' 'the Browser lists the 3D sketch under Sketches'
Ui-ExpectLog 'Browser sketch DOF: ' 'the Browser knows the sketches'' freedoms'
if ("$(Ui-LastMatch '^Browser sketch DOF: (.*)$')" -notmatch '3DSketch1 [0-9]+') {
  Ui-Fail "no degrees of freedom of 3DSketch1 in the Browser: $(Ui-LastMatch '^Browser sketch DOF: (.*)$')"
}
Write-Host 'ok   3DSketch1''s degrees of freedom in the Browser'

Write-Host '--- Create Sketch with 3D: an empty new 3D sketch'
Ui-Mark
Ui-Step 'create sketch (Alt+S)'          { Ui-Key 'alt+s' }
Ui-ExpectNew 'Create Sketch kind: 2D' '2D is the default'
Ui-ExpectNew 'Create Sketch kind 3D at' 'the 3D choice is on the card'
Ui-Mark
Ui-Step 'choose 3D'                      { Ui-ClickLogged 'Create Sketch kind 3D' }
Ui-ExpectNew 'Create Sketch kind: 3D' '3D chosen'
Ui-ExpectNew 'View banner: Create 3D Sketch: pick a plane' 'the banner names the kind'
Ui-SyncOr 500
Ui-Step 'pick XZ'                        { Ui-ClickLogged 'Datum xz' }
Ui-ExpectNew '3D sketch started on xz' 'a 3D sketch on XZ'
Ui-ExpectNew '(3DSketch2): 0 points, 0 curves' 'it is empty'
Ui-ExpectNew 'View banner: 3D Sketch 3DSketch2' 'the banner says it is a 3D sketch'
Ui-ExpectNew '3D sketch frame at ' 'its frame is drawn'
Ui-Mark
Ui-Step 'finish it empty (Ctrl+Enter)'   { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'
Ui-ExpectNew 'Browser sketch DOF: ' 'the Browser follows'
if ("$(Ui-LastMatch '^Browser sketch DOF: (.*)$')".Contains('3DSketch2')) {
  Ui-Fail 'the empty new 3D sketch stayed'
}
Write-Host 'ok   the empty new 3D sketch was taken back'

Write-Host '--- Edit Sketch of 3DSketch1'
Ui-Mark
Ui-Step 'edit 3DSketch1 (double-click)'  { Ui-DoubleClickLogged 'Timeline 3DSketch1' }
Ui-ExpectNew 'Editing 3D sketch F2 (3DSketch1)' '3DSketch1 is edited'
Ui-ExpectNew '3D sketch F2 (3DSketch1): ' 'its status is logged'
if ("$(Ui-LastMatch '^3D sketch frame in the design: (.*)$')" -notmatch '^origin \([-0-9.]+, [-0-9.]+, 20\.000000\)') {
  Ui-Fail "the frame is not on the top face: $(Ui-LastMatch '^3D sketch frame in the design: (.*)$')"
}
Write-Host 'ok   the frame follows the top face, 20 up'
Ui-ExpectNew 'sketch.slot: a 2D sketch tool does not apply to a 3D sketch' 'the planar-only tools are disabled'
Ui-ExpectNew '3D sketch entity F2/p2 at' 'its points are drawn where a click picks them'
Ui-ExpectNew '3D sketch edit controls across the card: ' 'the relations and dimensions are laid out on the card'
$across = Ui-LastLineWith '3D sketch edit controls across the card: '
if (-not "$across".EndsWith(': all whole')) { Ui-Fail "a relation or dimension button is cut at the card's edge: $across" }
Write-Host 'ok   every relation and dimension button is whole across the card (mitcad#1592)'
Ui-Mark
Ui-Step 'click the route''s end p2'      { Ui-ClickLogged '3D sketch entity F2/p2' }
Ui-ExpectNew 'Selected: 1 sketch point [sketch point p2 of F2]' 'p2 selected by its id'
Ui-ExpectNew 'selected Point p2 at 40.00, 0.00, 0.00 mm' 'the palette gives its local coordinates'
Ui-Sync
Ui-ExpectNew '3D sketch selection description ' 'the selection''s description is on the card'
$description = Ui-LastLineWith '3D sketch selection description '
if ("$description" -notmatch ' at \d+,\d+$') { Ui-Fail "the selected point's description is out of the card's sight: $description" }
Write-Host 'ok   the description is in the card''s sight (mitcad#1592)'
Ui-Mark
Ui-Step 'Esc (clear the selection)'      { Ui-Key 'Escape' }
Ui-ExpectNew 'Selected: nothing' 'Esc cleared the selection'
Ui-Step 'Esc again'                      { Ui-Key 'Escape' }
Ui-NotSince 'Sketch finished' 'Esc stays in the sketch'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'

Write-Host '--- The drawing tools (mitcad#1525): a Line from the origin by typed values'
Ui-Mark
Ui-Step 'create sketch (Alt+S)'          { Ui-Key 'alt+s' }
Ui-ExpectNew 'Create Sketch kind 3D at' 'the 3D choice is on the card'
Ui-Step 'choose 3D'                      { Ui-ClickLogged 'Create Sketch kind 3D' }
Ui-ExpectNew 'Create Sketch kind: 3D' '3D chosen'
Ui-SyncOr 500
Ui-Step 'pick XZ'                        { Ui-ClickLogged 'Datum xz' }
Ui-ExpectNew '3D sketch started on xz' 'a new 3D sketch on XZ'
Ui-ExpectNew '3D sketch frame at ' 'its frame is drawn'
Ui-Mark
Ui-Step 'Line (L)'                       { Ui-Key 'l' }
Ui-ExpectNew '3D sketch tool Line: started; on the XY plane' 'Line draws in the 3D sketch'
Ui-Mark
Ui-Step 'click the origin'               { Ui-ClickLogged '3D sketch frame' }
Ui-ExpectNew '3D sketch Line: pending point 1 at (0.000, 0.000, 0.000) (Origin)' 'the start is pending, at the origin'
Ui-NotSince '3D sketch command: ' 'nothing committed for the first point'
Ui-ExpectNew '3D sketch field dx at' 'the Offsets fields are shown'
Ui-Mark
Ui-Step 'type Delta X 40'                { Ui-ClickLogged '3D sketch field dx'; Ui-Type '40' }
Ui-ExpectNew '3D sketch field dx: 40' 'Delta X typed'
Ui-Step 'Tab to Delta Y'                 { Ui-Key 'Tab' }
Ui-ExpectNew '3D sketch field focus: dy' 'Tab reached Delta Y'
Ui-Step 'Tab to Delta Z'                 { Ui-Key 'Tab' }
Ui-ExpectNew '3D sketch field focus: dz' 'Tab reached Delta Z'
Ui-Step 'type Delta Z 0'                 { Ui-Type '0' }
Ui-ExpectNew '3D sketch field dz: 0' 'an explicit zero typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"offsets":["40' 'the typed values go as offsets'
Ui-ExpectNew '3D sketch Line: made c3 from p1 to p2' 'AB made, one step'
Ui-ExpectNew '2 points, 1 curves, dof 1' 'B free in Y only'
Ui-Mark
Ui-Step 'Esc (the field)'                { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch Esc: field dz back to its measurement' 'the field goes first'
Ui-Step 'Esc (the chain)'                { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch Line: chain ended' 'then the chain'
Ui-Step 'Esc (the tool)'                 { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch tool Line: stopped' 'then the tool'
Ui-NotSince 'Sketch finished' 'Esc stays in the sketch'

Write-Host '--- Center Point Arc: its normal picked from the line AB'
Ui-Mark
Ui-Step 'Center Point Arc (F8)'          { Ui-Key 'F8' }
Ui-ExpectNew '3D sketch tool Center Point Arc: started' 'Center Point Arc draws in the 3D sketch'
Ui-ExpectNew '3D sketch control Pick Normal at' 'its Normal row is shown'
Ui-Step 'Pick Normal'                    { Ui-ClickLogged '3D sketch control Pick Normal' }
Ui-ExpectNew '3D sketch Center Point Arc: picking the normal' 'the next click picks the normal'
$ab = Ui-LastMatch '3D sketch entity (F[0-9]+/c3) at'
if (-not $ab) { Ui-Fail 'the line AB (c3) is not logged where a click picks it' }
Ui-Step 'click AB'                       { Ui-ClickLogged "3D sketch entity $ab" }
Ui-ExpectNew '3D sketch Center Point Arc: normal from line c3 (1.000, 0.0' 'the arc''s normal along AB'
Ui-NotSince '3D sketch command: ' 'nothing committed by the pick'
Ui-Step 'Esc (the tool)'                 { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch tool Center Point Arc: stopped' 'the tool ends'

Write-Host '--- A line end snapped onto a face of the box (mitcad#1549): On Face'
Ui-Mark
Ui-Step 'Line (L)'                       { Ui-Key 'l' }
Ui-ExpectNew '3D sketch tool Line: started' 'Line draws again'
Ui-ExpectNew 'Pick face F1.b0/F1:side' 'the box''s faces are logged where a click meets them'
$face = Ui-LastMatch 'Pick face F1\.b0/(F1:side[0-9]+) at'
if (-not $face) { Ui-Fail 'no side face of the box to snap to' }
Ui-Mark
Ui-Step 'click the origin'               { Ui-ClickLogged '3D sketch frame' }
Ui-ExpectNew '3D sketch Line: pending point 1 at (0.000, 0.000, 0.000) (Point: p1)' 'the start is pending on AB''s start, the sketch''s point before the origin under it'
Ui-Mark
Ui-Step "click the face $face"           { Ui-ClickPick "face F1.b0/$face" }
Ui-ExpectNew "3D sketch snap: On Face: face $face of F1.b0" 'the face snaps, its badge names it'
Ui-ExpectNew "3D sketch snap badge On Face: face $face of F1.b0 at " 'the badge is placed'
$badge = Ui-LastLineWith '3D sketch snap badge On Face: '
if (-not "$badge".Contains(', inside the view ')) { Ui-Fail "the On Face badge is cut by the view: $badge" }
Write-Host 'ok   the On Face badge is whole inside the view (mitcad#1592)'
Ui-ExpectNew """end_source"":{""body"":""F1.b0"",""face"":""$face""}" 'the end goes onto the face'
Ui-ExpectNew '3D sketch Line: made c' 'the line made, one step'
Ui-Mark
Ui-Step 'Esc (the chain)'                { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch Line: chain ended' 'the chain ends'
Ui-Step 'Esc (the tool)'                 { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch tool Line: stopped' 'the tool ends'
Ui-ExpectNew ' On Face p' 'the On Face relation is in the list'
Ui-ExpectNew "face $face of F1.b0; body F1.b0; its point slides on the face" 'the face''s record is listed'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'

Write-Host '--- 3D sketch editing (mitcad#1526): a dimension typed in the list, a drag along X'
Ui-Mark
Ui-Step 'edit 3DSketch1 (double-click)'  { Ui-DoubleClickLogged 'Timeline 3DSketch1' }
Ui-ExpectNew 'Editing 3D sketch F2 (3DSketch1)' '3DSketch1 is edited again'
Ui-ExpectNew '3D sketch item k3 Length c3: 40.00 mm' 'the list gives AB''s length'
Ui-Mark
Ui-Step 'drag along X'                   { Ui-ClickLogged '3D sketch edit control drag axis X' }
Ui-ExpectNew '3D sketch drag on: along X' 'drags go along X'
# The card is taller than the 768 px screen: the list is below its sight
# until the card is scrolled (the wheel over its top, one step at a time,
# each logging the rows again where they are then).
Ui-Sync
Ui-Step 'scroll the card to the list (wheel)' {
  for ($i = 0; $i -lt 20; $i++) {
    if ("$(Ui-LastLineWith '3D sketch item k3 field ')" -match ' at \d+,\d+') { break }
    Ui-WheelLogged '3D Sketch Palette 3D Sketch' -1
    Start-Sleep -Milliseconds 400
    Ui-Sync
  }
}
Ui-ExpectNew '3D sketch item k3 field at' 'its value field is on the card'
# The card settled (its rows logged again where they are now) before the
# field's place is read.
Ui-Sync
Ui-Mark
Ui-Step 'AB''s length to 50 (W2)'         { Ui-ClickLogged '3D sketch item k3 field'; Ui-Key 'ctrl+a'; Ui-Type '50'; Ui-Key 'Return' }
Ui-ExpectNew '3D sketch edit value k3 = 50: done' 'the value set, one step'
Ui-ExpectNew '3D sketch item k3 Length c3: 50.00 mm' 'AB is 50 long'
Ui-ExpectNew '3D sketch entity F2/p2 at' 'B moved in the view'
Ui-ExpectNew '3D sketch item k3 remove at' 'the list laid out again, where Remove is now'
Ui-Mark
Ui-Step 'remove k3 (W3)'                 { Ui-ClickLogged '3D sketch item k3 remove' }
Ui-ExpectNew '3D sketch edit remove k3: done' 'the length removed: B is free along X'
Ui-Sync
Ui-Mark
$b = Ui-LoggedAt '3D sketch entity F2/p2'
Ui-Step 'drag B'                         { Ui-Drag $b[0] $b[1] ($b[0] + 60) $b[1] }
Ui-ExpectNew '3D sketch drag: started p2 along X' 'the drag started on B'
Ui-ExpectNew '3D sketch drag: p2 taken; p2 at' 'taken on release'
Ui-Mark
Ui-Step 'undo the drag'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Drag in 3DSketch1' 'one undo step'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'

Write-Host '--- Sketch1 turned into a 3D sketch in place (mitcad#1544)'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F3 (Sketch1)' 'Sketch1 is edited'
Ui-ExpectNew 'Palette palette3dSketch at' 'the Sketch Palette''s 3D Sketch box'
Ui-Mark
Ui-Step 'check 3D Sketch'                { Ui-ClickLogged 'Palette palette3dSketch' }
Ui-ExpectNew 'Palette: 3D Sketch checked for F3' 'the box asks for the conversion'
Ui-ExpectNew 'Converted Sketch1 (F3) to a 3D sketch: dof 0, in plane k1, k2, k3, k4, k5, k6' 'converted, fully constrained, kept in plane'
Ui-ExpectNew '3D Sketch Palette 3D Sketch at' 'the 3D Sketch Palette''s box'
$kind = Ui-LastLineWith '3D Sketch Palette 3D Sketch at'
if (-not "$kind".Contains('checked yes enabled yes: Uncheck to turn it back into a 2D sketch.')) {
  Ui-Fail "the 3D Sketch box is not checked and enabled with why: $kind"
}
Write-Host 'ok   checked, and enabled to turn it back (mitcad#1550)'
Ui-ExpectNew '3D sketch badge In plane k' 'the kept relations carry an In plane badge'
Ui-Sync
if ("$(Ui-LastLineWith '3D sketch badge In plane ')" -notmatch '^3D sketch badge In plane k\d+(,k\d+){6} at ') {
  Ui-Fail "not one In plane badge for the rectangle's six kept relations and its corner's in-plane Fix: $(Ui-LastLineWith '3D sketch badge In plane ')"
}
Write-Host 'ok   one In plane badge for the rectangle, its six kept relations and the in-plane Fix of its fixed corner (mitcad#1592)'
Ui-Mark
Ui-Step 'uncheck 3D Sketch'              { Ui-ClickLogged '3D Sketch Palette 3D Sketch' }
Ui-ExpectNew 'Converted Sketch1 (F3) to a 2D sketch: dof 0' 'back to 2D, fully constrained, in place'
Ui-ExpectNew 'Palette palette3dSketch at' 'the 2D editor takes it again'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Convert Sketch1 to 2D Sketch' 'one undo step'
Ui-ExpectNew 'Sketch editor follows the history: Sketch1 is a 3D sketch' 'the 3D editor again'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Convert Sketch1 to 3D Sketch' 'one undo step'
Ui-ExpectNew 'Sketch editor follows the history: Sketch1 is a 2D sketch' 'the 2D editor again'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'

Write-Host '--- Fillet (mitcad#1548): a corner of the triangle with a typed radius'
Ui-Mark
Ui-Step 'edit 3DSketch1 (double-click)'  { Ui-DoubleClickLogged 'Timeline 3DSketch1' }
Ui-ExpectNew 'Editing 3D sketch F2 (3DSketch1)' '3DSketch1 is edited for a fillet'
Ui-ExpectNew '3D sketch entity F2/p12 at' 'the triangle''s corner p12 is drawn'
Ui-Mark
Ui-Step 'Sketch Fillet (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew '3D sketch tool Fillet: started' 'Fillet runs in the 3D sketch'
Ui-ExpectNew '3D sketch field radius at' 'its Radius field is shown'
Ui-Mark
Ui-Step 'type Radius 3'                  { Ui-ClickLogged '3D sketch field radius'; Ui-Type '3' }
Ui-ExpectNew '3D sketch field radius: 3' 'Radius typed'
Ui-Mark
Ui-Step 'click the corner p12'           { Ui-ClickLogged '3D sketch entity F2/p12' }
Ui-ExpectNew '3D sketch Fillet: corner p12 of c13 and c15 picked' 'the corner picked by its point'
Ui-ExpectNew '"cmd":"sketch3d.fillet"' 'one sketch3d.fillet'
Ui-ExpectNew ' at p12, radius ' 'rounded at the typed radius at once, one step'
Ui-Mark
Ui-Step 'Esc'                            { Ui-Key 'Escape' }
Ui-ExpectNew '3D sketch Esc: ' 'Esc steps back'
Ui-Sync
if (-not ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('3D sketch tool Fillet: stopped') })) {
  # The field had the keyboard: its value went first.
  Ui-ExpectNew '3D sketch Esc: field radius back to its measurement' 'the field''s value goes first'
  Ui-Step 'Esc (the tool)'               { Ui-Key 'Escape' }
}
Ui-ExpectNew '3D sketch tool Fillet: stopped' 'the tool ends'
Ui-NotSince 'Sketch finished' 'Esc stays in the sketch'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'
Ui-Mark
Ui-Step 'undo'                           { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Edit 3DSketch1' 'the fillet''s edit is one step, the Timeline put back'

# Before the pipes: their bodies run round the route's curves, and a click
# on a curve inside one would meet the pipe's face first.
Write-Host '--- Rib from 3DSketch1''s curves (mitcad#1550): in its sketch plane taken, out of it refused'
Ui-Mark
Ui-Step 'Rib (Shift+F8)'                 { Ui-Key 'shift+F8' }
Ui-ExpectNew 'Sketch entity F2/c9 at' '3DSketch1''s curves are where a click picks them'
Ui-ExpectNew 'Sketch entity F2/c3 at' 'the line AB too'
Ui-SyncOr 500
Ui-Mark
Ui-Step 'the spline leaving the face'    { Ui-ClickLogged 'Sketch entity F2/c9' }
Ui-ExpectNew 'not taken: This curve of 3DSketch1 does not lie in its sketch plane' 'a curve out of the sketch plane is refused, saying why'
Ui-NotSince 'Rib Curves: 1 sketch curve' 'nothing taken'
# The refusal's hint in the status bar may lay the window out again: the
# line's place is read afresh.
Ui-Sync
Ui-Mark
Ui-Step 'the line AB in the plane'       { Ui-ClickLogged 'Sketch entity F2/c3' }
Ui-ExpectNew 'Rib Curves: 1 sketch curve [sketch curve c3 of F2]' 'a curve in the sketch plane is taken'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-SyncOr 500
if (-not ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Command Rib cancelled') })) {
  # A value field of the panel had the keyboard: its value went first.
  Ui-Step 'Esc (the command)'            { Ui-Key 'Escape' }
}
Ui-ExpectNew 'Command Rib cancelled' 'nothing added'

Write-Host '--- A Pipe along 3DSketch1''s route by one pick (mitcad#1527)'
Ui-Mark
Ui-Step 'Pipe (F9)'                      { Ui-Key 'F9' }
Ui-ExpectNew 'Panel Pipe input path at' 'the Pipe panel'
Ui-ExpectNew 'Sketch entity F2/c6 at' 'the route''s curves are where a click picks them'
Ui-SyncOr 500
Ui-Mark
Ui-Step 'pick the arc'                   { Ui-ClickLogged 'Sketch entity F2/c6' }
Ui-ExpectNew 'Pipe Path listed: c3, c6, c9 (p1 to p' 'one pick takes the whole route (Chain Selection), from A'
Ui-ExpectNew 'Preview Pipe: ok' 'previewed'
Ui-ExpectNew 'Panel Pipe input path_reverse at' 'Reverse is offered for a 3D sketch''s route'
if ("$(Ui-LastLineWith 'Panel Pipe input path_reverse at')".Contains('(hidden)')) { Ui-Fail 'Reverse is hidden' }
Write-Host 'ok   Reverse shown'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added pipe' 'the pipe added along the route'
Ui-Mark
Ui-Step 'edit Pipe1 (double-click)'      { Ui-DoubleClickLogged 'Timeline Pipe1' }
Ui-ExpectNew ' with Pipe' 'Pipe1 is edited'
Ui-ExpectNew 'Pipe Path listed: c3, c6, c9 (p1 to p' 'its route as picked'
Ui-ExpectNew 'Preview Pipe: ok' 'previewed as it is'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Pipe cancelled' 'nothing changed'

Write-Host '--- A Pipe round the closed triangle by one pick (mitcad#1550)'
Ui-Mark
Ui-Step 'Pipe (F9)'                      { Ui-Key 'F9' }
Ui-ExpectNew 'Panel Pipe input path at' 'the Pipe panel'
Ui-ExpectNew 'Sketch entity F2/c13 at' 'the triangle''s lines are where a click picks them'
Ui-SyncOr 500
Ui-Mark
Ui-Step 'pick a line of the triangle'    { Ui-ClickLogged 'Sketch entity F2/c13' }
Ui-ExpectNew 'Pipe Path listed: c13, ' 'one pick takes the triangle (Chain Selection)'
$listed = "$(Ui-LastLineWith 'Pipe Path listed: c13, ')"
if (-not $listed.Contains('(closed, from p11)')) { Ui-Fail "the triangle is not listed as closed from p11: $listed" }
Write-Host 'ok   listed as a closed route from p11'
Ui-ExpectNew 'Preview Pipe: ok' 'previewed round the triangle'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added pipe' 'Pipe2 added round the triangle'

Write-Host '--- Plane-bound tools (mitcad#1550): a rectangle from the origin, a circle on YZ'
Ui-Mark
Ui-Step 'create sketch (Alt+S)'          { Ui-Key 'alt+s' }
Ui-ExpectNew 'Create Sketch kind 3D at' 'the 3D choice is on the card'
Ui-Step 'choose 3D'                      { Ui-ClickLogged 'Create Sketch kind 3D' }
Ui-ExpectNew 'Create Sketch kind: 3D' '3D chosen'
Ui-SyncOr 500
Ui-Step 'pick XZ'                        { Ui-ClickLogged 'Datum xz' }
Ui-ExpectNew '3D sketch started on xz' 'a new 3D sketch on XZ'
Ui-ExpectNew '3D sketch frame at ' 'its frame is drawn'
Ui-Mark
Ui-Step '2-Point Rectangle (R)'          { Ui-Key 'r' }
Ui-ExpectNew '3D sketch tool 2-Point Rectangle: started; on the XY plane' 'the SKETCH tab''s rectangle draws in the 3D sketch'
Ui-Mark
Ui-Step 'click the origin'               { Ui-ClickLogged '3D sketch frame' }
Ui-ExpectNew '3D sketch 2-Point Rectangle: pending point 1 at (0.000, 0.000, 0.000) (Origin)' 'the first corner at the origin'
Ui-NotSince '3D sketch command: ' 'nothing committed for the first corner'
Ui-ExpectNew '3D sketch field width at' 'the Width field is shown'
Ui-Mark
Ui-Step 'type Width 30'                  { Ui-ClickLogged '3D sketch field width'; Ui-Type '30' }
Ui-ExpectNew '3D sketch field width: 30' 'Width typed'
Ui-Step 'Tab to Height'                  { Ui-Key 'Tab' }
Ui-ExpectNew '3D sketch field focus: height' 'Tab reached Height'
Ui-Step 'type Height 20'                 { Ui-Type '20' }
Ui-ExpectNew '3D sketch field height: 20' 'Height typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"cmd":"sketch3d.add_rectangle"' 'one sketch3d.add_rectangle'
Ui-ExpectNew '"plane":"xy"' 'on the active plane'
Ui-ExpectNew '"cmd":"sketch3d.attach"' 'the origin snap attaches its corner in the same step'
Ui-ExpectNew '3D sketch 2-Point Rectangle: made c' 'the rectangle made'
Ui-ExpectNew '4 points, 4 curves, dof 0' 'fully constrained by the origin, Width and Height'
Ui-Mark
Ui-Step 'Esc (the field)'                { Ui-Key 'Escape' }
Ui-SyncOr 500
if (-not ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('3D sketch tool 2-Point Rectangle: stopped') })) {
  # The field had the keyboard: its value went first.
  Ui-Step 'Esc (the tool)'               { Ui-Key 'Escape' }
}
Ui-ExpectNew '3D sketch tool 2-Point Rectangle: stopped' 'the tool ends'
Ui-Mark
Ui-Step 'undo'                           { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Rectangle in 3DSketch' 'the rectangle is one undo step'
Ui-Step 'redo'                           { Ui-Key 'ctrl+y' }
Ui-ExpectNew 'Redo: Add Rectangle in 3DSketch' 'and back'
Ui-Mark
Ui-Step 'Center Diameter Circle (C)'     { Ui-Key 'c' }
Ui-ExpectNew '3D sketch tool Center Diameter Circle: started' 'the circle draws in the 3D sketch'
Ui-ExpectNew '3D sketch control Plane YZ at' 'the plane choice is shown'
Ui-Step 'the YZ plane'                   { Ui-ClickLogged '3D sketch control Plane YZ' }
Ui-ExpectNew '3D sketch placement: on the YZ plane' 'the YZ plane through the anchor'
Ui-Mark
Ui-Step 'click the corner at the origin' { Ui-ClickLogged '3D sketch frame' }
Ui-ExpectNew '3D sketch Center Diameter Circle: pending point 1 at (0.000, 0.000, 0.000)' 'its centre on the corner'
Ui-ExpectNew '3D sketch field radius at' 'the Radius field is shown'
Ui-Mark
Ui-Step 'type Radius 8'                  { Ui-ClickLogged '3D sketch field radius'; Ui-Type '8' }
Ui-ExpectNew '3D sketch field radius: 8' 'Radius typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"cmd":"sketch3d.add_circle"' 'one sketch3d.add_circle'
Ui-ExpectNew '"plane":"yz"' 'bound to the YZ plane'
Ui-ExpectNew '3D sketch Center Diameter Circle: made c' 'the circle made on YZ'
Ui-ExpectNew '4 points, 5 curves, dof 0' 'its centre shared, its radius typed'
Ui-Mark
Ui-Step 'Esc (the field)'                { Ui-Key 'Escape' }
Ui-SyncOr 500
if (-not ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('3D sketch tool Center Diameter Circle: stopped') })) {
  Ui-Step 'Esc (the tool)'               { Ui-Key 'Escape' }
}
Ui-ExpectNew '3D sketch tool Center Diameter Circle: stopped' 'the tool ends'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'

Write-Host '--- Plane-bound tools (mitcad#1550): an ellipse, a polygon and a text in a dragged frame on XZ'
# Off-Origin dx dy: the pointer to a place of the view off the origin (left
# of the first sketch's rectangle and the box): a typed size goes toward the
# pointer, which a click on a field took out of the view.
function Off-Origin([int]$Dx, [int]$Dy) {
  $o = Ui-LoggedAt '3D sketch frame'
  [UiInput]::Move($script:UiWindow, $o[0] + $Dx, $o[1] + $Dy, 0)
  Ui-SyncOr 300
}
# End-Tool "name": Esc until the tool ends; the field that has the keyboard
# goes back first.
function End-Tool([string]$Name) {
  Ui-Mark
  Ui-Step 'Esc'                            { Ui-Key 'Escape' }
  Ui-SyncOr 500
  if (-not ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains("3D sketch tool ${Name}: stopped") })) {
    Ui-Step 'Esc (the tool)'               { Ui-Key 'Escape' }
  }
  Ui-ExpectNew "3D sketch tool ${Name}: stopped" "the $Name tool ends"
}
Ui-Mark
Ui-Step 'create sketch (Alt+S)'          { Ui-Key 'alt+s' }
Ui-ExpectNew 'Create Sketch kind 3D at' 'the 3D choice is on the card'
Ui-Step 'choose 3D'                      { Ui-ClickLogged 'Create Sketch kind 3D' }
Ui-ExpectNew 'Create Sketch kind: 3D' '3D chosen'
Ui-SyncOr 500
Ui-Step 'pick XZ'                        { Ui-ClickLogged 'Datum xz' }
Ui-ExpectNew '3D sketch started on xz' 'another 3D sketch on XZ'
Ui-ExpectNew '3D sketch frame at ' 'its frame is drawn'

Ui-Mark
Ui-Step 'Ellipse (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew '3D sketch tool Ellipse: started; on the XY plane' 'the SKETCH tab''s Ellipse draws in the 3D sketch'
Ui-Mark
Ui-Step 'click the origin'               { Ui-ClickLogged '3D sketch frame' }
Ui-ExpectNew '3D sketch Ellipse: pending point 1 at (0.000, 0.000, 0.000)' 'the center at the origin'
Ui-ExpectNew '3D sketch field major at' 'the Major Radius field is shown'
Ui-Step 'type Major Radius 10'           { Ui-ClickLogged '3D sketch field major'; Off-Origin -150 20; Ui-Type '10' }
Ui-ExpectNew '3D sketch field major: 10' 'Major Radius typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '3D sketch Ellipse: pending point 2 at' 'the major axis'' end 10 from the center'
Ui-NotSince '3D sketch command: ' 'nothing committed before the minor radius'
Ui-ExpectNew '3D sketch field minor at' 'the Minor Radius field is shown'
Ui-Step 'type Minor Radius 5'            { Ui-ClickLogged '3D sketch field minor'; Off-Origin -120 60; Ui-Type '5' }
Ui-ExpectNew '3D sketch field minor: 5' 'Minor Radius typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"cmd":"sketch3d.add_ellipse"' 'one sketch3d.add_ellipse'
Ui-ExpectNew '"plane":"xy"' 'on the active plane'
Ui-ExpectNew '"major_value":"10' 'the typed major radius drives'
Ui-ExpectNew '"minor_value":"5' 'the typed minor radius drives'
Ui-ExpectNew '3D sketch Ellipse: made c' 'the ellipse made'
Ui-ExpectNew ' points, 1 curves, dof ' 'the ellipse is the sketch''s curve'
End-Tool 'Ellipse'

Ui-Mark
Ui-Step 'Circumscribed Polygon (F5)'     { Ui-Key 'F5' }
Ui-ExpectNew '3D sketch tool Circumscribed Polygon: started; on the XY plane' 'the polygon draws in the 3D sketch'
Ui-Mark
Ui-Step 'click the origin'               { Ui-ClickLogged '3D sketch frame' }
Ui-ExpectNew '3D sketch Circumscribed Polygon: pending point 1 at (0.000, 0.000, 0.000)' 'its center at the origin'
Ui-ExpectNew '3D sketch field radius at' 'the Radius field is shown'
Ui-Step 'type Radius 20'                 { Ui-ClickLogged '3D sketch field radius'; Off-Origin -100 40; Ui-Type '20' }
Ui-ExpectNew '3D sketch field radius: 20' 'Radius typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"cmd":"sketch3d.add_polygon"' 'one sketch3d.add_polygon'
Ui-ExpectNew '"sides":6' 'six sides, the default'
Ui-ExpectNew '"radius":"20' 'the typed radius drives'
Ui-ExpectNew '3D sketch Circumscribed Polygon: made c' 'the polygon made'
Ui-ExpectNew ' points, 9 curves, dof ' 'six sides and two construction circles beside the ellipse'
End-Tool 'Circumscribed Polygon'

Ui-Mark
Ui-Step 'Text (F11)'                     { Ui-Key 'F11' }
Ui-ExpectNew '3D sketch tool Text: started; on the XY plane' 'the Text tool draws in the 3D sketch'
Ui-Mark
$o = Ui-LoggedAt '3D sketch frame'
# Above the origin, clear of the polygon and of the box; 220 to its left
# was over the Browser on Linux, which took the press.
Ui-Step 'drag a frame'                   { Ui-Drag ($o[0] - 60) ($o[1] - 150) ($o[0] + 20) ($o[1] - 110) }
Ui-ExpectNew '3D sketch Text: frame preview from (' 'the frame is previewed while it is dragged'
Ui-ExpectNew '3D sketch Text: frame to (' 'the release ends the frame'
Ui-ExpectNew '3D sketch Text: pending point 1 at' 'the text starts at the press'
Ui-NotSince '3D sketch command: ' 'nothing committed before the text is typed'
Ui-ExpectLog '3D sketch field text at' 'the Text field is shown (since the tool started)'
Ui-Sync # the card laid out again for the typing step (its places logged anew)
Ui-Step 'type the text'                  { Ui-ClickLogged '3D sketch field text'; Ui-Type 'MitCad' }
Ui-ExpectNew '3D sketch field text: MitCad' 'the text typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"cmd":"sketch3d.add_text"' 'one sketch3d.add_text'
Ui-ExpectNew '"frame":[' 'with its dragged frame'
Ui-ExpectNew '"text":"MitCad"' 'the text typed'
Ui-ExpectNew '3D sketch Text: made t' 'the text made'
Ui-ExpectNew ' points, 13 curves, dof ' 'its frame a construction rectangle'
End-Tool 'Text'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows 3D sketches'
