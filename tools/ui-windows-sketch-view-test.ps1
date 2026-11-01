# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/OcctViewer.cpp app/OcctViewer.hpp app/view app/MainWindow.cpp tools/cli
# check-all sources: app/framework/FloatingLayout.cpp
# Input-driven Windows UI test of how sketch mode shows its geometry in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted):
#   1. A sketch on XY: its origin is the design's (mitcad#254).
#   2. A rectangle from the origin, its width and height typed: fully
#      constrained.
#   3. The dark background recolours its fully constrained lines light, the
#      light one dark again (mitcad#253), their halos with them
#      (mitcad#302).
#   4. Clicks at known sketch points (no tool): the view reports their
#      sketch coordinates in mm (mitcad#254).
#   5. Outside sketch editing the shown sketch keeps the fully constrained
#      colour, also when the background changes; editing it again changes
#      no colour (mitcad#286).
#   6. A design (mitcad-cli) whose component's sketch, placed by its
#      occurrence, has a fully constrained rectangle and a free line: the
#      rectangle in the constrained colour, the line blue, in the main mode
#      and in its edit alike (mitcad#286), placed by the occurrence in both
#      (mitcad#505).
#   7. A design (mitcad-cli) with a block and a sketch on its top face with
#      three dimensions: each is drawn over a halo in the background's tone,
#      dark on the dark background and light on the light one, read from
#      the log ("Sketch dimension halos:"; mitcad#633; the pixels are
#      checked by tools/ui-sketch-view-test.sh).
#   8. A sketch on a block's top face, edited from the front view (the face
#      edge-on, the view's middle far in front of it in depth): the face's
#      middle in the middle of the free area (mitcad#772).
#   9. After New Design, sketches on XY, XZ and YZ show their origin in the
#      middle of the free area between the cards, in the Floating and the
#      Docked layout (mitcad#738: the turn kept the fit's shift).
#  10. Those sketches are seen from the view cube's TOP, FRONT and RIGHT
#      (mitcad#1060: XZ from FRONT, the design's +X to the right), on
#      entering them and after Isometric View and the Sketch Palette's
#      Look At.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-view-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-view'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'view.environment.dark' = 'F8'; 'view.environment.light' = 'F9';
  'view.front' = 'F11'; 'view.iso' = 'F12' } -StepPause 0

# Expect-Recoloured "dark|light": the sketch entities shown were recoloured
# for that background, the rectangle's four lines among the fully
# constrained ones.
function Expect-Recoloured([string]$Background) {
  Ui-ExpectNew "Sketch entities recoloured for a $Background background: " "recoloured for the $Background background"
  $count = Ui-LastMatch "Sketch entities recoloured for a $Background background: (\d+) fully constrained"
  if (-not $count -or [int]$count -lt 4) { Ui-Fail "$Background background: $count fully constrained entities recoloured, expected 4 or more" }
  Write-Host "ok   $count fully constrained entities recoloured for the $Background background"
  # Their halos took the background's tone too (mitcad#302).
  $halos = Ui-LastMatch 'Sketch halos: (\d+ (?:dark|light))$'
  if ($halos -notmatch "^(\d+) $Background$" -or [int]$Matches[1] -lt 4) {
    Ui-Fail "$Background background: the last halos are '$halos', expected 4 or more $Background"
  }
  Write-Host "ok   halos for the $Background background: $halos"
}

# Sketch-Colours "F1" (a regex: "F1 in O1", "F1(?: in O\d+)?"): the counts
# of the sketch's entities in the fully constrained colour and in the unconstrained blue, from the
# last "Sketch colours:" line, or $null when it does not name the sketch.
function Sketch-Colours([string]$Sketch) {
  $line = Ui-LogLines | Where-Object { $_.Contains('Sketch colours: ') } | Select-Object -Last 1
  $pattern = '(?:: |; )' + $Sketch + ' (\d+) fully constrained, (\d+) unconstrained(?:;|\s*$)'
  if ($line -and $line -match $pattern) { return @([int]$Matches[1], [int]$Matches[2]) }
  return $null
}

# Expect-Colours "F1" { param($constrained, $unconstrained) ... } "description":
# within six seconds the sketch's counts pass the check; returns them.
function Expect-Colours([string]$Sketch, [scriptblock]$Check, [string]$Description) {
  $counts = $null
  for ($i = 0; $i -lt 30; $i++) {
    $counts = Sketch-Colours $Sketch
    if ($counts -and (& $Check $counts[0] $counts[1])) {
      Write-Host "ok   ${Description}: $($counts[0]) fully constrained, $($counts[1]) unconstrained"
      return , $counts
    }
    Start-Sleep -Milliseconds 200
  }
  $last = Ui-LogLines | Where-Object { $_.Contains('Sketch colours: ') } | Select-Object -Last 1
  Ui-Fail "${Description}: the last colours are '$last'"
}

# Expect-Pointer x y: a click at the sketch point (x, y) mm with no tool
# active; the view reports that place, within two pixels. A click, not a
# move alone: a press always places the pointer (updateCursor), while a
# move posted in session 0 may be followed by a Leave at once.
function Expect-Pointer([double]$X, [double]$Y) {
  $at = Ui-SketchAt $X $Y
  $o = Ui-SketchAt 0 0
  $a = Ui-SketchAt 100 0
  $dx = $a[0] - $o[0]
  $dy = $a[1] - $o[1]
  $tolerance = 2 * 100 / [Math]::Sqrt($dx * $dx + $dy * $dy)
  Ui-Mark
  Ui-Step "click at ($X, $Y)" { Ui-Click $at[0] $at[1] }
  Ui-ExpectNew 'Sketch pointer x ' "the pointer's place at ($X, $Y) logged"
  $line = Ui-LogLines | Where-Object { $_.Contains('Sketch pointer x ') } | Select-Object -Last 1
  if ($line -notmatch 'Sketch pointer x (-?[0-9.]+), y (-?[0-9.]+) mm') { Ui-Fail "the pointer's place is not in mm: '$line'" }
  $px = [double]::Parse($Matches[1], [Globalization.CultureInfo]::InvariantCulture)
  $py = [double]::Parse($Matches[2], [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Abs($px - $X) -gt $tolerance -or [Math]::Abs($py - $Y) -gt $tolerance) {
    Ui-Fail "the view reports ($px, $py) for the pointer at ($X, $Y) (tolerance $tolerance mm)"
  }
  Write-Host "ok   the view reports x $px, y $py for the pointer at ($X, $Y)"
}

Write-Host '--- A sketch on XY, its origin the design origin (mitcad#254)'
Ui-StartApp 'sketch-view'
Ui-Step 'light background (F9)'          { Ui-Key 'F9' }
Ui-Step 'create sketch on XY (F7)'       { Ui-CreateSketch 'F7' 'xy' }
Ui-ExpectLog 'Create Sketch: origin planes shown' 'a new design: the origin planes shown (mitcad#844)'
Ui-ExpectLog 'Sketch view' 'the sketch view is logged'
Ui-ExpectLog 'Sketch origin at 0.00, 0.00, 0.00 mm in the design; the design origin at x 0.00, y 0.00 mm in the sketch' `
  'the sketch origin is the design origin'

Write-Host '--- A fully constrained rectangle on a light background'
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner at the origin'     { Ui-SketchClick 0 0 }
Ui-Step 'width 40 (Tab) height 25'       { Ui-Type '40'; Ui-Key 'Tab'; Ui-Type '25' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 40 x 25 mm at (0, 0)' 'rectangle 40 x 25 at the origin'
Ui-ExpectLog 'Sketch Sketch1: 0 DOF, fully constrained' 'fully constrained'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }

Write-Host '--- Fully constrained lines on a dark background (mitcad#253)'
Ui-Mark
Ui-Step 'dark background (F8)'           { Ui-Key 'F8' }
Expect-Recoloured 'dark'
Ui-Mark
Ui-Step 'light background (F9)'          { Ui-Key 'F9' }
Expect-Recoloured 'light'

Write-Host "--- The pointer's place in sketch coordinates (mitcad#254)"
Expect-Pointer 30 20
Expect-Pointer -15 -10

Write-Host '--- Outside sketch editing the sketch keeps its colours (mitcad#286)'
# A new design's first sketch goes into a new component, Component1 (O1;
# mitcad#220): outside its edit that occurrence places it.
$sketch1 = 'F1(?: in O\d+)?'
$editing = Expect-Colours $sketch1 { param($c, $u) $c -ge 4 -and $u -eq 0 } 'in its edit the rectangle is fully constrained'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-WaitIdle
$shown = Ui-LastMatch 'Sketches shown: (.*)$'
if ($shown -ne 'F1') { Ui-Fail "the finished sketch is not shown: 'Sketches shown: $shown'" }
Write-Host 'ok   the finished sketch is shown'
Expect-Colours $sketch1 { param($c, $u) $c -eq $editing[0] -and $u -eq $editing[1] } `
  'outside its edit the same colours, fully constrained' | Out-Null
Ui-Mark
Ui-Step 'dark background (F8)'           { Ui-Key 'F8' }
Expect-Recoloured 'dark'
Ui-Mark
Ui-Step 'light background (F9)'          { Ui-Key 'F9' }
Expect-Recoloured 'light'
Ui-ExpectLog 'Timeline Sketch1 at' 'the timeline is laid out'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited again' 30
Ui-WaitIdle
Expect-Colours $sketch1 { param($c, $u) $c -eq $editing[0] -and $u -eq $editing[1] } `
  'entering its edit changes no colour' | Out-Null
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished again'

Write-Host "--- A component's sketch placed by its occurrence (mitcad#286)"
# mitcad-cli builds it: Part:1 (O1) 100 mm along X, its Sketch1 (F1) a
# dimensioned 30 x 20 rectangle (fully constrained) and a free line (two
# degrees of freedom at each end).
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'component.json'
$design = Join-Path $work 'component.mitcad'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 30], "end": [20, 40]},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "set_occurrence_transform", "occurrence": "O1", "transform": {"translation": [100, 0, 0]}}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'sketch-view-component' @('--open', $design)
Ui-ExpectLog 'component.mitcad' 'the design opened'
Ui-ExpectLog 'Sketches shown: F1' "the component's sketch is shown"
$placed = Expect-Colours 'F1 in O1' { param($c, $u) $c -ge 4 -and $u -ge 1 } `
  'placed by Part:1, the rectangle fully constrained and the line blue'
Ui-Mark
Ui-Step 'dark background (F8)'           { Ui-Key 'F8' }
Expect-Recoloured 'dark'
Ui-Mark
Ui-Step 'light background (F9)'          { Ui-Key 'F9' }
Expect-Recoloured 'light'
Ui-ExpectLog 'Timeline Sketch1 at' 'the timeline is laid out'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' "the component's sketch is edited" 30
Ui-WaitIdle
# Edited where Part:1 places it too (mitcad#505), not at Part's own
# coordinates.
Expect-Colours 'F1 in O1' { param($c, $u) $c -eq $placed[0] -and $u -eq $placed[1] } `
  'in its edit placed by Part:1, the same colours' | Out-Null
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-WaitIdle
Expect-Colours 'F1 in O1' { param($c, $u) $c -eq $placed[0] -and $u -eq $placed[1] } `
  'placed by Part:1 again, the same colours' | Out-Null

Write-Host "--- Dimensions over a body's face have a halo in the background's tone (mitcad#633)"
# mitcad-cli builds a 60 x 40 x 30 mm block, a sketch on its top face (F2)
# with a circle (a diameter dimension) and a free line with a driven length
# dimension: three dimensions, each drawn over a halo (the pixels are
# checked by tools/ui-sketch-view-test.sh; this test reads the log).
$faceScript = Join-Path $work 'face.json'
$faceDesign = Join-Path $work 'face.mitcad'
Set-Content -Encoding ascii $faceScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-30, -20],
    "length": 60, "width": 40, "height": 30, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "F1:top", "body": "F1.b0"}},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [-28, 10], "end": [-4, 10]},
  {"cmd": "sketch.add_dimension", "sketch": "F2", "dimension": {"type": "length", "line": "c1"},
   "driven": true, "text": [12, 16]},
  {"cmd": "sketch.add_circle", "sketch": "F2", "center": [10, 0], "diameter": 16},
  {"cmd": "sketch.add_circle", "sketch": "F2", "center": [50, 0], "diameter": 16}
]
'@
$made = & $cli run $faceScript --save $faceDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $faceDesign)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'sketch-view-face' @('--open', $faceDesign)
Ui-ExpectLog 'face.mitcad' 'the design opened'
Ui-ExpectLog 'Sketches shown: F2' "the face's sketch is shown"
Ui-ExpectLog 'Timeline Sketch1 at' 'the timeline is laid out'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F2 (Sketch1)' "the face's sketch is edited" 30
Ui-WaitIdle

# Expect-DimensionHalos "dark|light": within six seconds the last "Sketch
# dimension halos:" line says the three dimensions have a halo of that tone.
function Expect-DimensionHalos([string]$Tone) {
  $last = $null
  for ($i = 0; $i -lt 30; $i++) {
    $last = Ui-LastMatch 'Sketch dimension halos: (\d+ (?:dark|light))$'
    if ($last -eq "3 $Tone") {
      Write-Host "ok   dimension halos: $last"
      return
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "dimension halos: the last line is '$last', expected '3 $Tone'"
}

Ui-Step 'dark background (F8)'           { Ui-Key 'F8' }
Expect-DimensionHalos 'dark'
Ui-Step 'light background (F9)'          { Ui-Key 'F9' }
Expect-DimensionHalos 'light'

Write-Host '--- A sketch on a face seen edge-on stays in view (mitcad#772)'
# mitcad-cli builds a 40 x 40 x 20 block far in front (y -400 to -360) and
# a 40 x 40 x 40 block (F2.b0, x 20 to 60, y 0 to 40) beside it, and a
# sketch (F3) on the second block's top face (z 40), which the front view
# sees edge-on. The view's middle lies half way between the blocks in
# depth: projected onto the top face's plane it is about 180 mm in front of
# the face, which the turn to the sketch plane took out of the view. Edit
# Sketch from the front view shows the middle of the face (40, 20, 40) in
# the middle of the free area instead.
$obliqueScript = Join-Path $work 'oblique.json'
$obliqueDesign = Join-Path $work 'oblique.mitcad'
Set-Content -Encoding ascii $obliqueScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-60, -400],
    "length": 40, "width": 40, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [20, 0],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "F2:top", "body": "F2.b0"}}
]
'@
$made = & $cli run $obliqueScript --save $obliqueDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $obliqueDesign)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'sketch-view-oblique' @('--open', $obliqueDesign)
Ui-ExpectLog 'oblique.mitcad' 'the design opened'
Ui-ExpectLog 'Bodies shown: F1.b0, F2.b0' 'the blocks shown'
Ui-Step 'front view (F11)'               { Ui-Key 'F11' }
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-WaitIdle
Ui-ExpectLog 'Timeline Sketch1 at' 'the timeline is laid out'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F3 (Sketch1)' "the top face's sketch is edited from the front" 30
Ui-ExpectNew "Sketch plane centred on the sketch's face: " 'the view turned about the middle of the face'
Ui-ExpectNew 'Sketch frame in the design' "the sketch's frame logged"
# Within six seconds the face's middle, from the sketch's frame (its origin,
# normal and x axis in the design) and the sketch view (where the sketch
# points (0, 0), (100, 0) and (0, 100) are in the window), is within 6 px
# of the middle of the free area.
$culture = [Globalization.CultureInfo]::InvariantCulture
$number = '(-?[0-9.]+)'
$triple = "\($number, $number, $number\)"
$frame = Ui-LogLines | Select-Object -Skip $script:UiMark |
  Where-Object { $_.Contains('Sketch frame in the design') } | Select-Object -Last 1
if ("$frame" -notmatch "origin $triple, normal $triple, x axis $triple") { Ui-Fail "the sketch's frame is not logged: '$frame'" }
$f = @(1..9 | ForEach-Object { [double]::Parse($Matches[$_], $culture) })
$o = $f[0..2]
$n = $f[3..5]
$xa = $f[6..8]
# The sketch's y axis: normal x x axis.
$ya = @(($n[1] * $xa[2] - $n[2] * $xa[1]), ($n[2] * $xa[0] - $n[0] * $xa[2]), ($n[0] * $xa[1] - $n[1] * $xa[0]))
$d = @((40 - $o[0]), (20 - $o[1]), (40 - $o[2]))
$sx = $d[0] * $xa[0] + $d[1] * $xa[1] + $d[2] * $xa[2]
$sy = $d[0] * $ya[0] + $d[1] * $ya[1] + $d[2] * $ya[2]
$centred = $false
$line = $null
$view = $null
for ($i = 0; $i -lt 30 -and -not $centred; $i++) {
  Ui-Sync
  $lines = Ui-LogLines | Select-Object -Skip $script:UiMark
  $line = $lines | Where-Object { $_.Contains('Sketch plane centred on ') } | Select-Object -Last 1
  $view = $lines | Where-Object { $_ -match 'Sketch view [-0-9.,]+ [-0-9.,]+ [-0-9.,]+' } | Select-Object -Last 1
  if ("$line" -match "Sketch plane centred on the sketch's face: origin at $number,$number; free area centre at $number,$number") {
    $fx = [double]::Parse($Matches[3], $culture)
    $fy = [double]::Parse($Matches[4], $culture)
    if ("$view" -match "Sketch view $number,$number $number,$number $number,$number") {
      $p = @(1..6 | ForEach-Object { [double]::Parse($Matches[$_], $culture) })
      $px = $p[0] + ($p[2] - $p[0]) * $sx / 100 + ($p[4] - $p[0]) * $sy / 100
      $py = $p[1] + ($p[3] - $p[1]) * $sx / 100 + ($p[5] - $p[1]) * $sy / 100
      $centred = ($px - $fx) * ($px - $fx) + ($py - $fy) * ($py - $fy) -le 36
    }
  }
  if (-not $centred) { Start-Sleep -Milliseconds 200 }
}
if (-not $centred) { Ui-Fail "the top face's middle is not in the middle of the free area: '$frame', '$line', '$view'" }
Write-Host "ok   the top face's middle in the middle of the free area ($line; $view)"
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' "the top face's sketch finished"

Write-Host "--- After New Design, a sketch's origin in the middle of the free area (mitcad#738)"
# Expect-Centred "description": since the mark the view turned to the new
# sketch's plane centred on its origin (a new design shows no bodies), and
# within six seconds the origin, as the view logs it and as the sketch logs
# it ("Sketch view"), is within 6 px of the middle of the free area between
# the cards.
function Expect-Centred([string]$Description) {
  Ui-ExpectNew 'Sketch plane centred on ' "${Description}: the view turned to the plane"
  $culture = [Globalization.CultureInfo]::InvariantCulture
  $line = $null
  $view = $null
  for ($i = 0; $i -lt 30; $i++) {
    Ui-Sync
    $lines = Ui-LogLines | Select-Object -Skip $script:UiMark
    $line = $lines | Where-Object { $_.Contains('Sketch plane centred on ') } | Select-Object -Last 1
    $view = $lines | Where-Object { $_ -match 'Sketch view [-0-9.,]+ [-0-9.,]+ [-0-9.,]+' } | Select-Object -Last 1
    if ("$line" -match 'Sketch plane centred on the origin: origin at (-?[0-9.]+),(-?[0-9.]+); free area centre at (-?[0-9.]+),(-?[0-9.]+)') {
      $ox = [double]::Parse($Matches[1], $culture)
      $oy = [double]::Parse($Matches[2], $culture)
      $fx = [double]::Parse($Matches[3], $culture)
      $fy = [double]::Parse($Matches[4], $culture)
      if ("$view" -match 'Sketch view (-?[0-9.]+),(-?[0-9.]+) ') {
        $sx = [double]::Parse($Matches[1], $culture)
        $sy = [double]::Parse($Matches[2], $culture)
        $logged = ($ox - $fx) * ($ox - $fx) + ($oy - $fy) * ($oy - $fy)
        $sketch = ($sx - $fx) * ($sx - $fx) + ($sy - $fy) * ($sy - $fy)
        if ($logged -le 36 -and $sketch -le 36) {
          Write-Host "ok   ${Description}: the origin in the middle of the free area ($line; $view)"
          return
        }
      }
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the origin is not in the middle of the free area: '$line', '$view'"
}

# Sketch-OnOrigin plane: Create Sketch on an origin plane, clicked in the
# view: Create Sketch shows the origin planes in every design (mitcad#844),
# also after New Design, which starts with its Component1 active
# (mitcad#898).
function Sketch-OnOrigin([string]$Plane) {
  $name = $Plane.ToUpperInvariant()
  Ui-Mark
  Ui-Key 'F7'
  Ui-ExpectNew 'Create Sketch: origin planes shown' 'Create Sketch shows the origin planes' | Out-Null
  Ui-ExpectNew "Datum $Plane at" "the $name plane can be picked" | Out-Null
  Start-Sleep -Milliseconds 300
  Ui-Sync
  Ui-ClickLogged "Datum $Plane"
  Ui-ExpectNew "Sketch started on $Plane" "sketch started on $Plane" | Out-Null
}

# Expect-Side "description" side "along": since the mark the view turned to
# the sketch plane from that side of the view cube, looking along that
# direction (mitcad#1060).
function Expect-Side([string]$Description, [string]$Side, [string]$Along) {
  Ui-ExpectNew "Sketch plane seen from $Side (along $Along)" "${Description}: seen from $Side"
}

# Expect-LookAtAgain "description" side "along": Isometric View turns the view
# away from the sketch, then the Sketch Palette's Look At turns it back to
# the same side.
function Expect-LookAtAgain([string]$Description, [string]$Side, [string]$Along) {
  Ui-Mark
  Ui-Step 'isometric view (F12)'          { Ui-Key 'F12' }
  Ui-ExpectNew 'Camera direction' "${Description}: the view turned away" 8
  Ui-ExpectLog 'Palette Look At at' "${Description}: the palette's Look At is laid out"
  Ui-Mark
  Ui-Step 'Look At (palette)'             { Ui-ClickLogged 'Palette Look At' }
  Expect-Side "$Description after Look At" $Side $Along
}

# Centred-Round layout chrome: New Design (its fit centres the design in the
# free area, which moves the camera's centre off the origin once the cards
# are laid out), then sketches on XY, XZ and YZ in turn; each shows its
# origin in the middle of the free area and is seen from the view cube's
# TOP, FRONT and RIGHT, also after Look At (mitcad#1060).
function Centred-Round([string]$Layout, [string]$Chrome) {
  Ui-StartApp "sketch-view-centred-$Chrome" @("--chrome=$Chrome")
  Ui-Mark
  Ui-Step "${Layout}: new design (Ctrl+N)" { Ui-Key 'ctrl+n' }
  Ui-ExpectNew 'New document' "${Layout}: a new design"
  Ui-ExpectNew 'Opened the new design with Component1 active' "${Layout}: the new design has Component1 active"
  Ui-WaitIdle
  foreach ($plane in @('xy', 'xz', 'yz')) {
    $name = $plane.ToUpperInvariant()
    Ui-Step "${Layout}: create sketch on $name (F7)" { Sketch-OnOrigin $plane }
    Expect-Centred "$Layout, $name"
    $side = @{ 'xy' = 'TOP'; 'xz' = 'FRONT'; 'yz' = 'RIGHT' }[$plane]
    $along = @{ 'xy' = '0 0 -1'; 'xz' = '0 1 0'; 'yz' = '-1 0 0' }[$plane]
    Expect-Side "$Layout, $name" $side $along
    Expect-LookAtAgain "$Layout, $name" $side $along
    Ui-Mark
    Ui-Step "${Layout}: finish sketch (Ctrl+Enter)" { Ui-Key 'ctrl+Return' }
    Ui-ExpectNew 'Sketch finished' "${Layout}: the $name sketch finished"
  }
}

Centred-Round 'Floating' 'floating'
Centred-Round 'Docked' 'docked'

Ui-Finish 'UI Windows sketch view test'
