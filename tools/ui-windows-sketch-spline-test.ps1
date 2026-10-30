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
# spline keeps its shape), and two points that widen the fitted view.
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
#   3. Extrude makes one body of it; saved, the file has the closed spline
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
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 50]}
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
Ui-ExpectNew 'Added closed spline c11 through 4 points' 'the click on the first point closed it'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
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
