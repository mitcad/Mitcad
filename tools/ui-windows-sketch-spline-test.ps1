# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# Input-driven Windows UI test of closed fitted splines and tangents at a
# chosen end (mitcad#783) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), as tools/ui-sketch-spline-test.sh does on Linux, on a
# design mitcad-cli makes: Sketch1 (F1) on XY with a fitted spline c1
# through p2 (-30, -30), p3 (-10, -20) and p4 (10, -30), a line c5 from
# (-70, -60) to its start p2 and a line c7 from its end p4 to (50, -60), each
# along the spline's end direction (the tangents hold as drawn, so the
# spline keeps its shape), two points that widen the fitted view, a fitted
# spline c11 through p12 (20, -20), p13 (35, -10), p14 (50, -20) and p15
# (65, -10), and one c16 through only p17 (-65, -15) and p18 (-45, -10).
#   0. Points over highlights (mitcad#897): the view draws the sketch's
#      points in a layer after the curves and their highlights, against
#      the scene's depth; the spline selected has its curve highlighted,
#      a fit point selected its ball, in that layer over the markers.
#      The overlay paints its own highlight of a selected curve (4 px,
#      over OCCT's scene) and then the curve's point markers over it, a
#      selected point as a ball over them (mitcad#946).
#   1. Tangent: the spline picked near its start and the first line, then
#      near its end and the second line: each tangent records the end it
#      was picked at (p2, p4), so one spline is tangent at both ends.
#   2. Fit Point Spline: four points and a click on the first one close it
#      into one smooth (periodic) spline, one profile.
#   3. Remove Point (mitcad#2326): the context menu's Remove Point takes p13
#      out of c11 and Delete on p14 alone takes it out too: c11 stays,
#      straight through its last two points. Delete on p18, a point of a
#      spline at its least two, deletes it with c16, as before.
#   4. Extrude makes one body of it; saved, the file has the closed spline
#      and both tangents' points.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-spline-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-spline'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.spline' = 'F7'; 'sketch.constraint.tangent' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'spline.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.spline", "sketch": "F1", "points": [[-30, -30], [-10, -20], [10, -30]]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [-70, -60], "end": "p2"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p4", "end": [50, -60]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-60, 50]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 50]},
  {"cmd": "sketch.spline", "sketch": "F1", "points": [[20, -20], [35, -10], [50, -20], [65, -10]]},
  {"cmd": "sketch.spline", "sketch": "F1", "points": [[-65, -15], [-45, -10]]}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'spline' @('--open', $design)
Ui-ExpectLog 'spline.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800

Write-Host '--- The points over the highlights of their curves'
Ui-ExpectLog "Sketch layers: halos < curves and highlights < points and point highlights; points keep the scene's depth; point highlights over the markers" `
  "the points are drawn after the curves' highlights, the model's depth still hides them"
Ui-Mark
Ui-Step 'select the spline'              { Ui-SketchClick -22 -24.32 }
Ui-ExpectNew 'Selected: 1 sketch curve' 'the spline selected'
Ui-ExpectNew 'Sketch highlights: 1 curve, 0 points' "its highlight in the curves' layer, under its fit points"
Ui-ExpectNew 'Sketch overlay: point markers over highlights; 1 curve, 0 points highlighted, 3 markers over them' `
  "the overlay paints the spline's three fit points over its highlight"
Ui-Mark
Ui-Step 'select its middle fit point'    { Ui-SketchClick -10 -20 }
Ui-ExpectNew 'Selected: 1 sketch point' 'the fit point selected'
Ui-ExpectNew 'Sketch highlights: 0 curves, 1 point' "its ball in the points' layer, over the marker"
Ui-ExpectNew 'Sketch overlay: point markers over highlights; 0 curves, 1 point highlighted, 0 markers over them' `
  "the overlay paints the fit point's ball"
Ui-Mark
# Off the x axis, which a click selects (mitcad#899).
Ui-Step 'click where nothing is'         { Ui-SketchClick 45 15 }
Ui-ExpectNew 'Selected: nothing' 'the selection cleared'

Write-Host '--- Tangent at each end of the spline'
# The spline is the natural cubic through its points: x = -30 + 40 t,
# y = -30 - 40 t^3 + 30 t on its first half, mirrored on the second; at
# t = 0.2 and 0.8 it is at (-22, -24.32) and (2, -24.32).
Ui-Mark
Ui-Step 'tangent (F8)'                   { Ui-Key 'F8' }
Ui-Step 'pick the spline near its start' { Ui-SketchClick -22 -24.32 }
Ui-Step 'pick the first line'            { Ui-SketchClick -50 -45 }
Ui-ExpectNew 'Added constraint tangent k' 'a tangent added'
Ui-ExpectNew 'Tangent would apply at p2' 'while picking, the glyph is at the end it will use'
Ui-ExpectNew ' on c1, c5 at p2' "at the spline's start, where it was picked"
Ui-Mark
Ui-Step 'pick the spline near its end'   { Ui-SketchClick 2 -24.32 }
Ui-Step 'pick the second line'           { Ui-SketchClick 30 -45 }
Ui-ExpectNew 'Tangent would apply at p4' 'while picking, the glyph is at the other end'
Ui-ExpectNew ' on c1, c7 at p4' "at the spline's end: tangent at both ends"
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
if (@(Ui-LogLines | Where-Object { $_.Contains('Constraint tangent refused') }).Count -gt 0) {
  Ui-Fail 'a tangent was refused'
}

Write-Host '--- A closed spline by a click on its first point'
Ui-Mark
Ui-Step 'fit point spline (F7)'          { Ui-Key 'F7' }
Ui-Step 'spline: 1'                      { Ui-SketchClick -50 20 }
Ui-Step 'spline: 2'                      { Ui-SketchClick -10 10 }
Ui-Step 'spline: 3'                      { Ui-SketchClick 30 20 }
Ui-Step 'spline: 4'                      { Ui-SketchClick -10 40 }
Ui-Step 'spline: the first point again'  { Ui-SketchClick -50 20 }
Ui-ExpectNew 'Fit Point Spline: a click here closes it' 'the first point is marked before the click'
Ui-ExpectNew 'Added closed spline c19 through 4 points' 'the click on the first point closed it'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }

Write-Host '--- Remove Point on a spline (mitcad#2326)'
# Expect-NotLogged "text" description: the log has no such line.
function Expect-NotLogged([string]$Text, [string]$Description) {
  if (@(Ui-LogLines | Where-Object { $_.Contains($Text) }).Count -gt 0) { Ui-Fail $Description }
}
Ui-Mark
$at = Ui-SketchAt 35 -10
Ui-Step 'right-click p13'                { Ui-Click $at[0] $at[1] 3 }
Ui-ExpectNew 'Selected: 1 sketch point' 'the right click selected the fit point'
Ui-ExpectNew 'Context menu: ' 'context menu opened'
$entries = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ')
$index = [array]::IndexOf($entries, 'Remove Point')
if ($index -lt 0) { Ui-Fail "no Remove Point in the context menu of a spline's fit point: $($entries -join ' | ')" }
Write-Host "ok   Remove Point offered for a spline's own point"
# The entries the arrow keys pass over (Ui-MenuSkipped) left out.
$downs = $index + 1 - (Ui-MenuSkipped $index)
Ui-Mark
Ui-Step 'choose Remove Point' {
  for ($i = 0; $i -lt $downs; $i++) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
Ui-ExpectNew 'Removed point p13 from c11' 'the point left the spline'
Expect-NotLogged 'Deleted p13' 'the spline was deleted with p13'
Ui-Mark
Ui-Step 'select p14'                     { Ui-SketchClick 50 -20 }
Ui-ExpectNew 'Selected: 1 sketch point' 'the fit point selected'
Ui-Mark
Ui-Step 'delete (Delete)'                { Ui-Key 'Delete' }
Ui-ExpectNew 'Removed point p14 from c11' 'Delete took the one point out of its spline'
Expect-NotLogged 'Deleted p14' 'the spline was deleted with p14'
# Two fit points left: the spline runs straight from (20, -20) to (65, -10).
Ui-Mark
Ui-Step "click the spline's middle"      { Ui-SketchClick 42.5 -15 }
Ui-ExpectNew 'Selected: 1 sketch curve' 'the spline stays'
Ui-Mark
Ui-Step 'select p18'                     { Ui-SketchClick -45 -10 }
Ui-ExpectNew 'Selected: 1 sketch point' 'the end of the two-point spline selected'
Ui-Mark
Ui-Step 'delete (Delete)'                { Ui-Key 'Delete' }
Ui-ExpectNew 'Remove Point refused: ' 'an open fitted spline keeps two points at least'
Ui-ExpectNew 'Deleted p18' 'Delete deleted the point with its spline, as before'
Ui-Mark
Ui-Step 'select p15'                     { Ui-SketchClick 65 -10 }
Ui-ExpectNew 'Selected: 1 sketch point' 'a point of the kept spline selected'
Ui-Mark
Ui-Step 'click where c16 was'            { Ui-SketchClick -55 -12.5 }
Ui-ExpectNew 'Selected: nothing' 'c16 is gone'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'

Write-Host '--- One profile, one body'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with the closed profile'
Ui-Step 'type the distance 10'           { Ui-Type '10' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'New body Body1 (F2.b0): volume' 'the closed spline extruded as one profile'
if (@(Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }).Count -gt 0) { Ui-Fail 'a recompute failed' }

Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$saved = [IO.File]::ReadAllText($design)
if ($saved -notmatch '"closed": ?true') { Ui-Fail 'the saved spline is not closed' }
Write-Host 'ok   the saved spline is closed'
if ($saved -notmatch '"at": ?"p2"' -or $saved -notmatch '"at": ?"p4"') {
  Ui-Fail "the saved tangents do not have their points"
}
Write-Host "ok   the saved tangents have their points"

Ui-Finish 'UI Windows sketch spline test'
