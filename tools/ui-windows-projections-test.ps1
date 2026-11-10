# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp app/sketch
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: app/framework/SelectionScope.cpp core/model/src/features/sketch.rs
# Input-driven Windows UI test of Clean Up Projections (mitcad#232) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted):
#   1. A 40 x 25 block 20 high; a sketch on its front face.
#   2. Project (P) the front face twice, linked: two projections of the same
#      four edges.
#   3. Clean Up Projections (a shortcut of the test's): the panel's preview
#      lists the repeated curves it removes; OK keeps it, and running it
#      again has nothing to clean up.
#   4. Clean Up Stale Projections (mitcad#252, a shortcut of the test's): both
#      projections follow the face, so nothing is stale.
#   5. A design (mitcad-cli) whose sketch holds a projection of an earlier
#      shape of a face (stale): the Sketch Palette offers Clean Up Stale
#      Projections under its warning, which is a link: a click selects the
#      projection's 8 curves and points (mitcad#264), again the same, Esc
#      clears them. Project Again (mitcad#1303) beside it starts its command,
#      whose preview projects the face again; Esc leaves the sketch as it was.
#      Clean Up Stale Projections started from there, the preview
#      highlights the curve it removes and the lines it trims, Esc takes the
#      highlights away; again (F9) and OK keeps it, the highlights gone.
#   6. A design (mitcad-cli, mitcad#287): a rectangle standing on a projected
#      line, its bottom Horizontal and its left corner on the line's left
#      end. Coincident (a shortcut of the test's) on its right corner and
#      the line's right end is added, the Horizontal it implies removed, and
#      the hint says so; one undo takes both back. The Point tool (a
#      shortcut of the test's) on a derived offset curve: the point is
#      added, the coincident of its snap is refused, and the hint shows why.
#   7. A design (mitcad-cli, mitcad#736): a block and a sketch on XZ through
#      it. Intersect (a shortcut of the test's) takes the block picked in the
#      browser: its section, linked. Extrude1 made 30 high: the sketch has
#      no warning and the section's corner is at the new top.
#   8. A design (mitcad-cli, mitcad#904): Sketch1 with a line and a circle
#      on XY, Sketch2 on XY after it. Project in Sketch2: its selection
#      filter has the row for other sketches' curves and points, a click
#      takes Sketch1's line, and OK logs it by name.
#   9. The same design (mitcad#1031): Sketch1 also has a circle round
#      (30, 0) of radius 5, and Sketch3 lies on XZ. Intersect in Sketch3: its
#      selection filter has the row for other sketches' curves and points,
#      Sketch1 picked in the browser goes in whole, and OK gives two fixed
#      points where its circle crosses XZ (x = 25 and 35); the saved design
#      has them as one linked section of the circle.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-projections-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-projections'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'sketch.clean_up_projections' = 'F8'
  'sketch.clean_up_stale_projections' = 'F9'; 'sketch.constraint.coincident' = 'Shift+F5'; 'sketch.point' = 'Shift+F6'
  'sketch.intersect' = 'Shift+F7' } -StepPause 0

Write-Host '--- A block and a sketch on its front face'
Ui-StartApp 'projections'
Ui-Step 'create sketch on XY (F7)'       { Ui-CreateSketch 'F7' 'xy' }
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner at the origin'     { Ui-SketchClick 0 0 }
Ui-Step 'width 40 (Tab) height 25'       { Ui-Type '40'; Ui-Key 'Tab'; Ui-Type '25' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 40 x 25 mm at (0, 0)' 'rectangle 40 x 25 at the origin'
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectLog 'Sketch finished' 'sketch finished'

Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with the new profile'
Ui-Step 'type the distance 20'           { Ui-Type '20' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (New Body), distance 20' 'extrusion built'

Ui-Mark
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F2.b0/F2:side(c1[c4,c2])' 'the front face can be picked'
Ui-Step 'pick the front face'            { Ui-ClickPick 'face F2.b0/F2:side(c1[c4,c2])' }
Ui-ExpectNew 'Sketch started on {"body":"F2.b0","face":"F2:side(c1[c4,c2])' 'a sketch on the front face'

Write-Host '--- The front face projected twice'
foreach ($time in @('once', 'twice')) {
  Ui-Mark
  Ui-Step "project (P), $time"           { Ui-Key 'p' }
  Ui-ExpectNew 'Pick face F2.b0/F2:side(c1[c4,c2])' 'the front face can be picked'
  Ui-Step 'pick the front face'          { Ui-ClickPick 'face F2.b0/F2:side(c1[c4,c2])' }
  Ui-ExpectNew 'Preview Project: ok' 'projection previewed'
  Ui-Step 'OK (Enter)'                   { Ui-Key 'Return' }
  Ui-ExpectNew 'Projected 1 item(s)' "projected $time"
}

Write-Host '--- Clean Up Projections'
Ui-Mark
Ui-Step 'clean up projections (F8)'      { Ui-Key 'F8' }
Ui-ExpectNew 'Preview Clean Up Projections: ok, warning: curves the same as another removed: ' `
  'the preview lists the repeated curves'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Cleaned up projections' 'kept'

Ui-Mark
Ui-Step 'clean up projections again (F8)' { Ui-Key 'F8' }
Ui-ExpectNew 'nothing to clean up' 'nothing left to clean up'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Write-Host '--- Clean Up Stale Projections'
Ui-Mark
Ui-Step 'clean up stale projections (F9)' { Ui-Key 'F9' }
Ui-ExpectNew 'nothing to clean up: every linked projection matches its source' 'nothing stale'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Write-Host '--- A sketch whose projection no longer matches its source (mitcad#252)'
# mitcad-cli builds it: a 60 x 40 block 20 high with a fillet of 5 on its
# vertical edge at (60, 0); a sketch on XY holds the block's bottom face as
# an earlier version recorded it (no by_geometry): 70 wide from x = -10,
# sharp corners. A line of the user's at x = 20 ends on its bottom and top
# edges, and an extrude uses the region right of it. The face has an arc
# now, so the projection keeps its geometry, warns, and all its curves are
# stale.
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'stale.json'
$design = Join-Path $work 'stale.mitcad'
Set-Content -Encoding ascii $script (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [-10, 0], "fixed": true, "reference": true},
      {"id": "p2", "type": "point", "at": [60, 0], "fixed": true, "reference": true},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
      {"id": "p4", "type": "point", "at": [60, 40], "fixed": true, "reference": true},
      {"id": "c5", "type": "line", "start": "p2", "end": "p4", "fixed": true, "reference": true},
      {"id": "p6", "type": "point", "at": [-10, 40], "fixed": true, "reference": true},
      {"id": "c7", "type": "line", "start": "p4", "end": "p6", "fixed": true, "reference": true},
      {"id": "c8", "type": "line", "start": "p6", "end": "p1", "fixed": true, "reference": true},
      {"id": "p9", "type": "point", "at": [20, 0]},
      {"id": "p10", "type": "point", "at": [20, 40]},
      {"id": "c11", "type": "line", "start": "p9", "end": "p10"}],
    "constraints": [
      {"id": "k12", "type": "coincident", "point": "p9", "entity": "c3"},
      {"id": "k13", "type": "coincident", "point": "p10", "entity": "c7"}],
    "projections": [{"source": "F2:start(RECT)",
                     "entities": ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]}]}},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c3[c11,c5],c5[c3,c7],c7[c5,c11],c11[c3,c7]}"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}}
]
'@ -creplace 'RECT', 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}')
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'stale' @('--open', $design)
Ui-ExpectLog 'stale.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch2 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F4 (Sketch2)' 'the sketch is edited' 30
Ui-ExpectNew 'Palette paletteCleanUpStale at' 'the palette offers the clean-up under the warning'

Write-Host '--- The warning selects the curves it is about (mitcad#264)'
Ui-ExpectLog 'Palette warning 0 at' 'the palette shows the warning'
$look = @(Ui-LogLines | Where-Object { $_.Contains('Palette warning 0 at ') }) | Select-Object -Last 1
if (-not $look.EndsWith(': a link (pointing hand), tooltip "Click to select the curves this is about"')) {
  Ui-Fail "the warning does not look like a link: $look"
}
Write-Host 'ok   the warning looks like a link'
Ui-Mark
Ui-Step 'click the warning'              { Ui-ClickLogged 'Palette warning 0' }
Ui-ExpectNew 'Palette: warning 0 selects 8 entities: p1, p2, c3, p4, c5, p6, c7, c8' `
  'the click selects the projection''s curves and points'
Ui-ExpectNew 'Selected: 4 sketch curves, 4 sketch points' 'the selection holds the 8 of them'
Ui-Mark
Ui-Step 'click the warning again'        { Ui-ClickLogged 'Palette warning 0' }
Ui-ExpectNew 'Palette: warning 0 selects 8 entities' 'a second click selects them again'
Ui-Mark
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Ui-ExpectNew 'Selected: nothing' 'the selection is cleared'

Write-Host '--- Project Again from the warning (mitcad#1303)'
Ui-ExpectLog 'Palette paletteProjectAgain at' 'the palette offers Project Again beside the clean-up'
Ui-Mark
Ui-Step 'project again from the palette' { Ui-ClickLogged 'Palette paletteProjectAgain' }
Ui-ExpectNew 'Palette: Project Again from the warning' 'the warning started the command'
Ui-ExpectNew 'Preview Project Again: ok' 'the preview projects the face again'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Project Again cancelled' 'the sketch is left stale for the clean-up'

Write-Host '--- Clean Up Stale Projections from the warning'
Ui-Mark
Ui-Step 'clean up from the palette'      { Ui-ClickLogged 'Palette paletteCleanUpStale' }
Ui-ExpectNew 'Palette: Clean Up Stale Projections from the warning' 'the warning started the command'
Ui-ExpectNew 'Preview highlights: removed c8; trimmed c3, c7' 'the preview highlights what goes and what is trimmed'
# The shown warning names no curves (mitcad#1612); the highlights above do.
Ui-ExpectNew 'Preview Clean Up Stale Projections: ok, warning: 1 stale curve removed: a curve;' 'the preview lists it in words'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Preview highlights: none' 'cancel takes the highlights away'

Write-Host '--- Clean Up Stale Projections kept'
Ui-Mark
Ui-Step 'clean up stale projections (F9)' { Ui-Key 'F9' }
Ui-ExpectNew 'Preview highlights: removed c8; trimmed c3, c7' 'the preview highlights them again'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Cleaned up stale projections' 'kept'
Ui-ExpectNew 'Preview highlights: none' 'OK takes the highlights away'

Write-Host '--- A partly implied coincident replaces the Horizontal (mitcad#287)'
# Sketch1 (F1) on XY: a projected line (c3, fixed reference geometry, from
# p1 at (0, 0) to p2 at (60, 0)); a rectangle on it (c8 its bottom from p4
# at (0, 0) to p5 at (40, 0), Horizontal k12), its left corner coincident
# with p1 (k16); and above it a closed outline of three lines and a spline
# offset out by 2 (derived: its lower line at y = 58 cannot be constrained).
$script = Join-Path $work 'implied.json'
$design = Join-Path $work 'implied.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
    {"id": "p1", "type": "point", "at": [0, 0], "fixed": true, "reference": true},
    {"id": "p2", "type": "point", "at": [60, 0], "fixed": true, "reference": true},
    {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": true, "reference": true},
    {"id": "p4", "type": "point", "at": [0, 0]},
    {"id": "p5", "type": "point", "at": [40, 0]},
    {"id": "p6", "type": "point", "at": [40, 25]},
    {"id": "p7", "type": "point", "at": [0, 25]},
    {"id": "c8", "type": "line", "start": "p4", "end": "p5"},
    {"id": "c9", "type": "line", "start": "p5", "end": "p6"},
    {"id": "c10", "type": "line", "start": "p6", "end": "p7"},
    {"id": "c11", "type": "line", "start": "p7", "end": "p4"},
    {"id": "p17", "type": "point", "at": [0, 60]},
    {"id": "p18", "type": "point", "at": [40, 60]},
    {"id": "c19", "type": "line", "start": "p17", "end": "p18"},
    {"id": "p20", "type": "point", "at": [40, 80]},
    {"id": "c21", "type": "line", "start": "p18", "end": "p20"},
    {"id": "p22", "type": "point", "at": [20, 88]},
    {"id": "p23", "type": "point", "at": [0, 80]},
    {"id": "c24", "type": "fitted_spline", "points": ["p20", "p22", "p23"]},
    {"id": "c25", "type": "line", "start": "p23", "end": "p17"}],
    "constraints": [
      {"id": "k12", "type": "horizontal", "line": "c8"},
      {"id": "k13", "type": "vertical", "line": "c9"},
      {"id": "k14", "type": "horizontal", "line": "c10"},
      {"id": "k15", "type": "vertical", "line": "c11"},
      {"id": "k16", "type": "coincident", "point": "p4", "entity": "p1"}]}},
  {"cmd": "sketch.offset", "sketch": "F1", "curves": ["c19", "c21", "c24", "c25"], "distance": 2}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# The sketch's degrees of freedom the app logged last.
function Sketch-Dof {
  $dof = Ui-LastMatch 'Sketch Sketch1: (\d+) DOF'
  if (-not $dof) { Ui-Fail 'the app did not log the sketch''s degrees of freedom' }
  return [int]$dof
}

Ui-StartApp 'implied' @('--open', $design)
Ui-ExpectLog 'implied.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-Mark
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Sync
$before = Sketch-Dof
Ui-Mark
Ui-Step 'coincident (Shift+F5)'          { Ui-Key 'shift+F5' }
Ui-ExpectNew 'Sketch tool Coincident' 'Coincident active'
Ui-Step 'pick the right bottom corner'   { Ui-SketchClick 40 0 }
Ui-Step 'pick the projected line''s end' { Ui-SketchClick 60 0 }
Ui-ExpectNew 'Removed constraint horizontal k12: implied by the new coincident k' `
  'the Horizontal the coincident implies is removed'
Ui-ExpectNew ' on p5, p2' 'the coincident is added'
Ui-ExpectNew 'Sketch notice: Horizontal removed: implied by the new Coincident.' 'the hint says so'
Ui-Sync
if ((Sketch-Dof) -ne $before - 1) { Ui-Fail "$(Sketch-Dof) DOF, expected $($before - 1)" }
Write-Host 'ok   the corner on the line''s end, one degree of freedom fewer'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: ' 'undone'
Ui-Sync
if ((Sketch-Dof) -ne $before) { Ui-Fail "$(Sketch-Dof) DOF after the undo, expected $before" }
Write-Host 'ok   one undo takes both back'

Write-Host '--- A snapped coincident that is refused says why (mitcad#287)'
Ui-Mark
Ui-Step 'point (Shift+F6)'               { Ui-Key 'shift+F6' }
Ui-ExpectNew 'Sketch tool Point' 'Point active'
Ui-Step 'click on the derived offset'    { Ui-SketchClick 10 58 }
Ui-ExpectNew 'Added point ' 'the point is added'
Ui-ExpectNew 'Constraint coincident on ' 'the snap''s coincident is logged'
Ui-ExpectNew ' not added: ' 'refused, with the reason'
Ui-ExpectNew 'Sketch notice: Coincident not added: ' 'the hint shows the reason'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectLog 'Sketch finished' 'sketch finished'

Write-Host '--- Intersect: the section of a body follows it (mitcad#736)'
# mitcad-cli builds it: a 40 x 25 block 20 high from (-20, -10); Sketch2
# (F3) on XZ cuts it through (x along X, y along -Z).
$script = Join-Path $work 'intersect.json'
$design = Join-Path $work 'intersect.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-20, -10], "width": 40, "height": 25},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'intersect' @('--open', $design)
Ui-ExpectLog 'intersect.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch2 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F3 (Sketch2)' 'Sketch2 edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Sync
Ui-Mark
Ui-Step 'intersect (Shift+F7)'           { Ui-Key 'shift+F7' }
Ui-ExpectLog 'Browser Root/Bodies/Body1 at' 'Body1 in the browser'
Ui-Step 'pick Body1 in the browser'      { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Intersect Geometry: 1 body' 'the body went to Intersect'
Ui-ExpectNew 'Preview Intersect: ok' 'the section previewed' 30
Ui-WaitIdle
Ui-FocusMain
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Intersected 1 item(s)' 'the section kept'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch2 finished'

Ui-Mark
Ui-Step 'edit Extrude1 (double-click)'   { Ui-DoubleClickLogged 'Timeline Extrude1' }
Ui-ExpectNew ' with Extrude' 'editing Extrude1' 30
Ui-Step 'distance 30'                    { Ui-TypeIn 'Panel Extrude input distance' '30' }
Ui-ExpectNew 'Preview Extrude: ok' 'the edit previewed' 30
Ui-WaitIdle
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F2' 'Extrude1 edited' 30
if ((Ui-LogLines) -match 'Recompute failed') { Ui-Fail 'a recompute failed' }

Ui-Mark
Ui-Step 'edit Sketch2 again'             { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F3 (Sketch2)' 'Sketch2 edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Sync
# The lines since the mark (Ui-Mark is the index of the log's last, still
# empty line: the first new line is there).
$since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
if ($since -match 'Palette warning') { Ui-Fail 'Sketch2 warns: the section did not follow the block' }
Write-Host 'ok   Sketch2 has no warning: the section followed the block'
# The section's top right corner is at x = 20, z = 30 now: (20, -30).
Ui-Mark
$from = Ui-SketchAt 20 -30
$to = Ui-SketchAt 26 -36
Ui-Step 'drag the section''s new corner' { Ui-Drag $from[0] $from[1] $to[0] $to[1] }
Ui-Sync
$since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
if (-not ($since -match 'Drag of p[0-9]+ did not move it|Dragged p[0-9]+ to \(20\.000, -30\.000\)')) {
  Ui-Fail 'no section point at the block''s new top corner'
}
Write-Host 'ok   the section''s corner is where the block is now'
# The old top line ran along z = 20: nothing of the section is there now.
Ui-Mark
$from = Ui-SketchAt 0 -20
$to = Ui-SketchAt 6 -26
Ui-Step 'drag at the old top'            { Ui-Drag $from[0] $from[1] $to[0] $to[1] }
Ui-Sync
$since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
if ($since -match 'Drag of |Dragged ') { Ui-Fail 'a section curve stayed at the old top' }
Write-Host 'ok   nothing of the section is left at the old top'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch2 finished'
if ((Ui-LogLines) -match 'Recompute failed') { Ui-Fail 'a recompute failed' }
Write-Host '--- Project: a curve of an earlier sketch (mitcad#904)'
# Sketch1 (F1) on XY: a line c3 from (-20, 10) to (20, 10), a circle c5
# round (0, -10) and a circle c7 round (30, 0) of radius 5, which crosses
# XZ; Sketch2 (F2) on XY after it and Sketch3 (F3) on XZ, empty.
$script = Join-Path $work 'sketches.json'
$design = Join-Path $work 'sketches.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
    {"id": "p1", "type": "point", "at": [-20, 10]},
    {"id": "p2", "type": "point", "at": [20, 10]},
    {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
    {"id": "p4", "type": "point", "at": [0, -10]},
    {"id": "c5", "type": "circle", "center": "p4", "radius": 8},
    {"id": "p6", "type": "point", "at": [30, 0]},
    {"id": "c7", "type": "circle", "center": "p6", "radius": 5}]}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.create", "plane": "xz"}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'sketches' @('--open', $design)
Ui-ExpectLog 'sketches.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'edit Sketch2 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F2 (Sketch2)' 'Sketch2 edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Sync
Ui-Mark
Ui-Step 'project (P)'                    { Ui-Key 'p' }
Ui-ExpectNew 'Command Project started' 'Project started'
Ui-ExpectNew 'other-sketch-geometry=cross' 'Project''s filter has the row for other sketches'' geometry'
Ui-ExpectNew 'Sketch entity F1/c3 at' 'Sketch1''s line can be picked'
Ui-Step 'pick Sketch1''s line'           { Ui-ClickLogged 'Sketch entity F1/c3' }
Ui-ExpectNew 'Project Geometry: 1 sketch curve [sketch curve c3 of F1]' 'the line went to Project'
Ui-ExpectNew 'Preview Project: ok' 'the projection previewed' 30
Ui-WaitIdle
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Projected 1 item(s): sketch curve c3 of F1' 'the log names the projected curve'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch2 finished'
if ((Ui-LogLines) -match 'Recompute failed') { Ui-Fail 'a recompute failed' }

Write-Host '--- Intersect: where another sketch''s circle crosses the plane (mitcad#1031)'
# Sketch3 (F3) on XZ: x along X, y along -Z. The circle c7 crosses it at
# (25, 0, 0) and (35, 0, 0), seen edge on as a line from x = 25 to 35.
Ui-Mark
Ui-Step 'edit Sketch3 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch3' }
Ui-ExpectNew 'Editing sketch F3 (Sketch3)' 'Sketch3 edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Sync
Ui-Mark
Ui-Step 'intersect (Shift+F7)'           { Ui-Key 'shift+F7' }
Ui-ExpectNew 'other-sketch-geometry=cross' 'Intersect''s filter has the row for other sketches'' geometry'
# In the view the circle is seen edge on, and Sketch1 is hidden once
# Sketch2 projects it: Sketch1 is picked whole in the browser (its line and
# other circle do not cross XZ).
Ui-ExpectLog 'Browser Root/Sketches/Sketch1 at' 'Sketch1 in the browser'
Ui-Step 'pick Sketch1 in the browser'    { Ui-ClickLogged 'Browser Root/Sketches/Sketch1' }
Ui-ExpectNew 'Intersect Geometry: 1 sketch' 'Sketch1 went to Intersect'
Ui-ExpectNew 'Preview Intersect: ok' 'the crossings previewed' 30
Ui-WaitIdle
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Intersected 1 item(s): sketch F1' 'the log names the intersected sketch'
# The fit before Intersect saw nothing of Sketch3: the points come in view.
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Sync
foreach ($x in @(25, 35)) {
  Ui-Mark
  $from = Ui-SketchAt $x 0
  $to = Ui-SketchAt ($x + 3) -6
  Ui-Step "drag the crossing at x = $x"  { Ui-Drag $from[0] $from[1] $to[0] $to[1] }
  Ui-Sync
  $since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
  if (-not ($since -match "Drag of p[0-9]+ did not move it|Dragged p[0-9]+ to \($x\.000, 0\.000\)")) {
    Ui-Fail "no fixed point where the circle crosses XZ at x = $x"
  }
  Write-Host "ok   a fixed point where the circle crosses XZ at x = $x"
}
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch3 finished'
if ((Ui-LogLines) -match 'Recompute failed') { Ui-Fail 'a recompute failed' }
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
# Two points only, one linked section of the circle, no warning.
$check = Join-Path $work 'check-intersect.json'
Set-Content -Encoding ascii $check @'
[
  {"expect": {"query": {"query": "sketch", "uid": "F3"}, "result": {
    "entities": [{"type": "point"}, {"type": "point"}],
    "projections": [{"section": true, "source": {"sketch": "F1", "curve": "c7"}}],
    "warnings": []}}}
]
'@
$checked = & $cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved crossings: $($checked -join ' / ')" }
Write-Host 'ok   two linked points where the circle crosses XZ'
Ui-Finish 'UI Windows projections test'
