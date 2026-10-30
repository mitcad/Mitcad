# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/ModelShapes.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver tools/cli
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/MainWindowSelection.cpp app/OcctViewer.cpp
# Input-driven Windows UI test of the sketch's axes and origin as picks of
# Sketch Dimension and the constraint tools (mitcad#500) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-sketch-axes-test.sh on Linux:
#   1. A design (mitcad-cli): Sketch1 with a free point p1 at (20, 12), a
#      free line c2 from (40, 5) to (70, 20) and a free point p5 at
#      (-8, 15); nothing at the origin (8 DOF).
#   2. Sketch Dimension: p1, then the x axis: 12.00 offered, 30 typed: the
#      x axis made for the pick, p1 at 30 mm from it (7 DOF).
#   3. Sketch Dimension: the line, then the x axis, the value between
#      them: 26.57 offered, 25 typed: the line at 25 degrees (6 DOF).
#   4. Coincident (Shift+F5 in the test's settings): p5, then the y axis:
#      the y axis made, p5 on it (5 DOF); undo takes both back, redo makes
#      them again.
#   5. The saved design: the axes' lines, the values, p5 at x = 0.
#   6. Mirror about the y axis (mitcad#876; Shift+F6 in the test's
#      settings): Sketch2 has a free point at (-8, 5) and a free circle
#      about (10, 12) radius 3. The circle selected, Mirror opens with
#      Mirror Line active; a click on the y axis picks it, the preview and
#      OK mirror the circle to (-10, 12), the y axis made in the same step;
#      the saved sketch has the copy symmetric about the axis.
#   7. The axes selected before a tool (mitcad#899): Sketch3 has free
#      points p1 at (-6, 8) and p2 at (14, -3), which the fit shows with
#      the axes between them (4 DOF). A click on p1 and a Ctrl+click on
#      the x axis select both with no tool running; Sketch Dimension
#      offers p1's distance from the x axis, 8.00, and 5 typed makes the
#      x axis and the dimension in one step (3 DOF). p1 and the y axis
#      selected the same way, which makes nothing, Coincident makes the y
#      axis in its step and puts p1 on it (2 DOF); undo takes both back.
#   8. Drawing onto an axis (mitcad#948): Sketch4 is empty. Hovering the
#      x axis with the circle tool makes nothing; a circle whose centre is
#      clicked on the x axis makes the x axis and holds the centre on it
#      with a Coincident in the circle's step (2 DOF); one undo takes all
#      of it back (0 DOF).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-axes-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-axes'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Coincident and Mirror have no default shortcut (the command search's
# popup closes at once in session 0).
Ui-Init $App $Out $QtBin @{ 'sketch.constraint.coincident' = 'Shift+F5'; 'sketch.mirror' = 'Shift+F6' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'axes.json'
$design = Join-Path $work 'axes.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [20, 12]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [40, 5], "end": [70, 20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-8, 15]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F2", "at": [-8, 5]},
  {"cmd": "sketch.circle", "sketch": "F2", "mode": "center", "center": [10, 12], "radius": 3},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [-6, 8]},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [14, -3]},
  {"cmd": "sketch.create"}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Ui-ZoomOut steps: wheel steps towards the user at the view's middle, then
# a moment for the new "Sketch view" line.
function Ui-ZoomOut([int]$Steps) {
  $at = Ui-ViewAt 50 50
  for ($i = 0; $i -lt $Steps; $i++) {
    [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0)
    Start-Sleep -Milliseconds 80
    [UiInput]::Wheel($script:UiWindow, $at[0], $at[1], -1)
    Start-Sleep -Milliseconds 300
  }
  Start-Sleep -Milliseconds 800
}

Ui-StartApp 'axes' @('--open', $design)
Ui-ExpectLog 'axes.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-ExpectNew 'Sketch Sketch1: 8 DOF' 'two free points and a free line'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800
Ui-Step 'zoom out to show the axes'      { Ui-ZoomOut 2 }

Write-Host '--- 1. A point 30 mm from the x axis'
Ui-Mark
Ui-Step 'sketch dimension (D)'           { Ui-Key 'd' }
Ui-ExpectNew 'Sketch tool Sketch Dimension' 'Sketch Dimension active'
Ui-Step 'pick p1'                        { Ui-SketchClick 20 12 }
Ui-Step 'pick the x axis'                { Ui-SketchClick 8 0 }
Ui-Step 'place the value'                { Ui-SketchClick 26 6 }
Ui-ExpectNew 'Sketch value editor at' 'the value editor opened'
Ui-ExpectNew ' with 12.00' "p1's distance from the x axis offered"
Ui-Step 'type 30, Enter'                 { Ui-Key 'ctrl+a'; Ui-Type '30'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew 'Made the sketch x axis for a pick: c7' 'the x axis made for the pick'
Ui-ExpectNew 'Added dimension k' 'a driving dimension'
Ui-ExpectNew 'point_line_distance' "the point's distance from the axis"
Ui-ExpectNew 'Sketch Sketch1: 7 DOF' 'p1 held at its distance'

Write-Host '--- 2. A line at 25 degrees to the x axis'
Ui-Mark
Ui-Step 'pick the line'                  { Ui-SketchClick 55 12.5 }
Ui-Step 'pick the x axis'                { Ui-SketchClick 8 0 }
Ui-Step 'place the value between them'   { Ui-SketchClick 45 3 }
Ui-ExpectNew 'Sketch value editor at' 'the value editor opened'
Ui-ExpectNew ' with 26.57' 'the angle between them offered'
Ui-Step 'type 25, Enter'                 { Ui-Key 'ctrl+a'; Ui-Type '25'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew 'Added dimension k' 'a driving dimension'
Ui-ExpectNew ': 25.00 deg, the arc from 25.0 to 0.0 deg' 'the line turned to 25 degrees'
Ui-ExpectNew 'Sketch Sketch1: 6 DOF' "the line's direction held"
if (@(Ui-LogLines | Where-Object { $_.Contains('Made the sketch x axis') }).Count -ne 1) {
  Ui-Fail 'the x axis was made twice'
}
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }

Write-Host '--- 3. A point on the y axis'
Ui-Mark
Ui-Step 'coincident (Shift+F5)'          { Ui-Key 'shift+F5' }
Ui-ExpectNew 'Sketch tool Coincident' 'Coincident active'
Ui-Step 'pick p5'                        { Ui-SketchClick -8 15 }
Ui-Step 'pick the y axis'                { Ui-SketchClick 0 25 }
Ui-ExpectNew 'Made the sketch y axis for a pick: c9' 'the y axis made for the pick'
Ui-ExpectNew 'Added constraint coincident k' 'the point on the axis'
Ui-ExpectNew 'on p5, c9' "p5 and the y axis's line"
Ui-ExpectNew 'Sketch Sketch1: 5 DOF' 'p5 on the y axis'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Sketch Sketch1: 6 DOF' 'the constraint and the y axis taken back as one step'
Ui-Mark
Ui-Step 'redo (Ctrl+Y)'                  { Ui-Key 'ctrl+y' }
Ui-ExpectNew 'Sketch Sketch1: 5 DOF' 'both again'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'

Write-Host '--- 4. The saved design'
# The entities in the order they were made: p1, the line's points and the
# line, p5, then the origin point, the x axis's far end and line, the y
# axis's far end and line.
$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @'
[
  {"expect": {"sketch": "Sketch1", "dof": 5}},
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "entities": [
      {"id": "p1", "at": [20, 30]},
      {"id": "p3"}, {"id": "p4"}, {"id": "c2"},
      {"id": "p5", "at": [0, 15]},
      {"id": "p6", "at": [0, 0], "fixed": true},
      {"id": "p8", "axis": "x", "fixed": true},
      {"id": "c7", "type": "line", "start": "p6", "end": "p8", "axis": "x", "construction": true, "fixed": true},
      {"id": "p10", "axis": "y", "fixed": true},
      {"id": "c9", "type": "line", "start": "p6", "end": "p10", "axis": "y", "construction": true, "fixed": true}],
    "constraints": [{"type": "coincident", "point": "p5", "entity": "c9"}],
    "dimensions": [{"type": "point_line_distance", "point": "p1", "line": "c7", "measured": 30},
                   {"type": "angle", "a": "c2", "b": "c7", "measured": 0.4363323129985824}],
    "regions": []}}}
]
'@
$checked = & $cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved axes, dimensions and constraint: $($checked -join ' / ')" }
Write-Host 'ok   the axes, dimensions and constraint saved'

Write-Host '--- 5. Mirror a circle about the y axis (mitcad#876)'
Ui-Mark
Ui-Step 'edit Sketch2 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F2 (Sketch2)' 'Sketch2 is edited' 30
Ui-ExpectNew 'Sketch Sketch2: 5 DOF' 'a free point and a free circle'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800
Ui-Step 'zoom out to show the axes'      { Ui-ZoomOut 2 }
Ui-Mark
Ui-Step 'select the circle'              { Ui-SketchClick 10 15 }
Ui-ExpectNew 'Selected: 1 sketch curve' 'the circle selected'
Ui-Mark
Ui-Step 'mirror (Shift+F6)'              { Ui-Key 'shift+F6'; Ui-Sync }
Ui-Step 'pick the y axis'                { Ui-SketchClick 0 18 }
Ui-ExpectNew 'Sketch axis picked for the command: y_axis (y axis)' 'the y axis picked as the mirror line'
Ui-ExpectNew 'Mirror Mirror Line: 1 sketch curve [sketch curve y_axis of F2]' 'Mirror Line holds the y axis'
Ui-ExpectNew 'Preview Mirror: ok' 'the mirror previewed'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Mirrored 1 object(s) about the y axis' 'the circle mirrored about the y axis'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch2 finished'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
# The point, the circle's centre and the circle, then what the mirror
# made in its step: the origin point, the y axis's far end and line, the
# copy's centre and the copy.
$checkMirror = Join-Path $work 'check-mirror.json'
Set-Content -Encoding ascii $checkMirror @'
[
  {"expect": {"sketch": "Sketch2", "dof": 5}},
  {"expect": {"query": {"query": "sketch", "uid": "F2"}, "result": {
    "entities": [
      {"id": "p1", "at": [-8, 5]},
      {"id": "p3", "at": [10, 12]},
      {"id": "c2", "type": "circle", "center": "p3", "radius": 3},
      {"id": "p4", "at": [0, 0], "fixed": true},
      {"id": "p6", "axis": "y", "fixed": true},
      {"id": "c5", "type": "line", "start": "p4", "end": "p6", "axis": "y", "construction": true, "fixed": true},
      {"id": "p8", "at": [-10, 12]},
      {"id": "c7", "type": "circle", "center": "p8", "radius": 3}],
    "constraints": [{"type": "symmetric", "a": "c2", "b": "c7", "axis": "c5"}]}}}
]
'@
$mirrored = & $cli run $checkMirror --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved mirror about the y axis: $($mirrored -join ' / ')" }
Write-Host 'ok   the copy saved symmetric about the y axis'

# Ui-CtrlSketchClick x y: a click at a sketch point (mm) with Ctrl held,
# adding to the selection.
function Ui-CtrlSketchClick([double]$X, [double]$Y) {
  $at = Ui-SketchAt $X $Y
  Ui-ClickWith 'ctrl' $at[0] $at[1]
}

Write-Host '--- 7. The axes selected before a tool: Sketch Dimension, then Coincident (mitcad#899)'
Ui-Mark
Ui-Step 'edit Sketch3 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch3' }
Ui-ExpectNew 'Editing sketch F3 (Sketch3)' 'Sketch3 is edited' 30
Ui-ExpectNew 'Sketch Sketch3: 4 DOF' 'two free points'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800
Ui-Step 'zoom out to show the axes'      { Ui-ZoomOut 2 }
Ui-Mark
Ui-Step 'select p1'                      { Ui-SketchClick -6 8 }
Ui-ExpectNew 'Selected: 1 sketch point' 'p1 selected'
Ui-Mark
Ui-Step 'and the x axis (Ctrl)'          { Ui-CtrlSketchClick 8 0 }
Ui-ExpectNew 'Sketch axis selected: x_axis (x axis)' 'the x axis selected with no tool running'
Ui-ExpectNew 'Selected: 1 sketch curve, 1 sketch point [sketch point p1 of F3; sketch curve x_axis of F3]' 'p1 and the x axis in the selection'
Ui-Mark
Ui-Step 'sketch dimension (D)'           { Ui-Key 'd' }
Ui-ExpectNew 'Sketch tool Sketch Dimension' 'Sketch Dimension takes the selection'
Ui-Step 'place the value'                { Ui-SketchClick -12 2 }
Ui-ExpectNew 'Sketch value editor at' 'the value editor opened'
Ui-ExpectNew ' with 8.00' "p1's distance from the x axis offered"
Ui-Step 'type 5, Enter'                  { Ui-Key 'ctrl+a'; Ui-Type '5'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew 'Made the sketch x axis for a pick: c' "the x axis made in the dimension's step"
Ui-ExpectNew 'point_line_distance' "p1's distance from the x axis"
Ui-ExpectNew 'Sketch Sketch3: 3 DOF' 'p1 held 5 mm from the x axis'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'select p1'                      { Ui-SketchClick -6 5 }
Ui-ExpectNew 'Selected: 1 sketch point' 'p1 selected again'
Ui-Mark
Ui-Step 'and the y axis (Ctrl)'          { Ui-CtrlSketchClick 0 7 }
Ui-ExpectNew 'Sketch axis selected: y_axis (y axis)' 'the y axis selected with no tool running'
Ui-ExpectNew 'Selected: 1 sketch curve, 1 sketch point [sketch point p1 of F3; sketch curve y_axis of F3]' 'p1 and the y axis in the selection'
if (@(Ui-NewLines | Where-Object { $_.Contains('Made the sketch') }).Count -ne 0) {
  Ui-Fail 'selecting the y axis made it'
}
Ui-Mark
Ui-Step 'coincident (Shift+F5)'          { Ui-Key 'shift+F5' }
Ui-ExpectNew 'Made the sketch y axis for a pick: c' "the y axis made in the constraint's step"
Ui-ExpectNew 'Added constraint coincident k' 'p1 on the y axis'
Ui-ExpectNew 'on p1, c' "p1 and the y axis's line"
Ui-ExpectNew 'Sketch Sketch3: 2 DOF' 'p1 held on the y axis'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Sketch Sketch3: 3 DOF' 'the constraint and the y axis taken back as one step'
Ui-Mark
Ui-Step 'redo (Ctrl+Y)'                  { Ui-Key 'ctrl+y' }
Ui-ExpectNew 'Sketch Sketch3: 2 DOF' 'both again'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch3 finished'

Write-Host "--- 8. A circle's centre snapped onto the x axis (mitcad#948)"
Ui-Mark
Ui-Step 'edit Sketch4 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch4' }
Ui-ExpectNew 'Editing sketch F4 (Sketch4)' 'Sketch4 is edited' 30
Ui-ExpectNew 'Sketch Sketch4: 0 DOF' 'an empty sketch'
Ui-Step 'zoom out to show the axes'      { Ui-ZoomOut 2 }
Ui-Mark
Ui-Step 'circle tool (C)'                { Ui-Key 'c' }
Ui-ExpectNew 'Sketch tool Center Diameter Circle' 'the circle tool'
Ui-Step 'hover the x axis'               {
  $at = Ui-SketchAt 4 0
  [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0)
  Start-Sleep -Milliseconds 300
  Ui-SyncOr 500
}
if (@(Ui-NewLines | Where-Object { $_.Contains('Made the sketch') }).Count -ne 0) {
  Ui-Fail 'hovering the x axis made it'
}
Write-Host 'ok   hovering the x axis makes nothing'
Ui-Mark
Ui-Step 'centre on the x axis'           { Ui-SketchClick 10 0 }
Ui-Step 'rim'                            { Ui-SketchClick 10 6 }
Ui-ExpectNew 'Added circle, diameter' 'the circle added'
Ui-ExpectNew 'Made the sketch x axis for a pick: c' "the x axis made in the circle's step"
Ui-ExpectNew ' on the sketch x axis c' 'the centre held on the x axis by a Coincident'
Ui-ExpectNew 'Sketch Sketch4: 2 DOF' "only the circle's x and radius free"
if (@(Ui-NewLines | Where-Object { $_ -match 'Constraint coincident on .* not added' }).Count -ne 0) {
  Ui-Fail 'the Coincident with the axis was refused'
}
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Sketch Sketch4: 0 DOF' 'the circle, the x axis and the Coincident taken back as one step'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch4 finished'

Ui-Finish 'UI Windows sketch axes test'
