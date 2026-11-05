# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-drawing/src/annotate.rs core/model/src/drawing/annotate_def.rs
# check-all sources: core/model-api/src/drawing_spacing.rs core/model-api/src/drawing_annotations.rs tools/cli
# Input-driven Windows UI test of the Dimension tool's picks choosing the
# dimension, the Centerline of a curved slot and the Arc tool's snap
# (mitcad#1272) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), on a plate like the report's that mitcad-cli makes: 90 x 50 x
# 5, two holes of 16 at (30, 25) and (62, 32), small ones of 4, and a
# curved slot 4 wide about (56, 42) on the radius 26 from 245 to 295
# degrees; a drawing of it with its top view at 2:1, so that the plate's
# point (x, y) lies at (120 + 2x, 100 + 2y) on the sheet:
#   1. Dimension (Type Auto): one click on a hole and its place make its
#      diameter.
#   2. A hole, then the plate's right edge while the hole's dimension is
#      placed, placed between them diagonally: aligned, 28 from the hole's
#      centre to the edge; the preview says so before the click.
#   3. Centerline on the slot's two sides: its centre arc.
#   4. Arc, the sheet zoomed in: its centre clicked on the slot's centre,
#      its start and end on the centres of the slot's ends; each snaps
#      there, so the arc is the slot's centre arc.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-dimension-picks-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-dimension-picks'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# A rectangle's region; the arc slot's: its outer side c1 (from the cap
# at its start, c4, to the cap at its end, c2), c2, its inner side c3
# (also from c4 to c2: each piece's ends are named in its own direction)
# and c4 (sketch.slot's `arc` mode on an empty sketch).
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$slot = 'r{c1[c4,c2],c2[c1,c3],c3[c4,c2],c4[c3,c1]}'

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'plate.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 90, "height": 50},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 25], "diameter": 16},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [62, 32], "diameter": 16},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F7", "center": [30, 42], "diameter": 4},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F9", "center": [30, 8], "diameter": 4},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F9", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.slot", "sketch": "F11", "mode": "arc", "center": [56, 42],
    "start": [45.0118, 18.4360], "end": [66.9882, 18.4360], "width": 4},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F11", "region": "SLOT"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F2", "status": "ok"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"expect": {"feature": "F6", "status": "ok"}},
  {"expect": {"feature": "F8", "status": "ok"}},
  {"expect": {"feature": "F10", "status": "ok"}},
  {"expect": {"feature": "F12", "status": "ok"}},
  {"expect": {"bodies": 1}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "2:1"}
]
'@ -creplace 'RECT', $region -creplace 'SLOT', $slot)
# The plate and its cuts built (a feature that failed, the slot's region
# not found say, fails here with the report, not later on an empty view).
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Write-Host "ok   the plate made: $(@($made | Select-Object -First 12) -join ' / ')"

$invariant = [Globalization.CultureInfo]::InvariantCulture

# Sheet-At x y: the window point of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)).
function Sheet-At([double]$X, [double]$Y) {
  $view = Ui-LastMatch 'Drawing sheet view ([-0-9.,]+ [-0-9.,]+ [-0-9.,]+)'
  if (-not $view) { Ui-Fail 'the app did not log the drawing''s sheet' }
  $p = $view -split ' ' | ForEach-Object { , ($_ -split ',' | ForEach-Object { [double]::Parse($_, $invariant) }) }
  $px = $p[0][0] + ($p[1][0] - $p[0][0]) * $X / 100 + ($p[2][0] - $p[0][0]) * $Y / 100
  $py = $p[0][1] + ($p[1][1] - $p[0][1]) * $X / 100 + ($p[2][1] - $p[0][1]) * $Y / 100
  return @([int][Math]::Round($px), [int][Math]::Round($py))
}

# Plate-At x y: the window point of a point of the plate (model mm), in
# its top view at 2:1, whose box (the plate, 90 x 50) has its middle at
# the logged "Drawing view 1 at x,y": the sheet point 210 + 2 (x - 45),
# 150 + 2 (y - 25) when the view lies where the script put it. Said in
# the output, so that a failed pick shows where it was clicked.
function Plate-At([double]$X, [double]$Y) {
  $middle = Ui-LastMatch 'Drawing view 1 at (-?[0-9]+,-?[0-9]+)'
  if (-not $middle) { Ui-Fail 'the app did not log where view 1 is' }
  $m = $middle -split ',' | ForEach-Object { [double]::Parse($_, $invariant) }
  $origin = Sheet-At 0 0
  $unit = Sheet-At 100 100
  # Window pixels per sheet mm along x and y (y down in the window).
  $sx = ($unit[0] - $origin[0]) / 100.0
  $sy = ($unit[1] - $origin[1]) / 100.0
  $px = [int][Math]::Round($m[0] + $sx * 2 * ($X - 45))
  $py = [int][Math]::Round($m[1] + $sy * 2 * ($Y - 25))
  $expected = Sheet-At (210 + 2 * ($X - 45)) (150 + 2 * ($Y - 25))
  Write-Host "     plate $X,$Y -> window $px,$py (view 1's middle at $middle; at its scripted place: $($expected[0]),$($expected[1]))"
  return @($px, $py)
}

# Plate-Click x y: a click at a point of the plate.
function Plate-Click([double]$X, [double]$Y) {
  $at = Plate-At $X $Y
  Ui-Click $at[0] $at[1]
  Ui-Sync
}

# Plate-Hover x y: the pointer moved there while a dimension is placed:
# its preview and snap follow it (placingMoved). As in
# tools/ui-windows-drawing-spacing-test.ps1: a move with nothing in wParam
# is dropped by Qt in session 0, MK_SHIFT keeps it a plain move. What a
# click would pick (the sheet's hover) is not waited for here: session 0
# has no cursor, so Qt follows each posted move with a leave that clears
# the hover at once ("Drawing hover: nothing at 0.0,0.0 on view -1"); the
# clicks' picks and the model tests (drawing_pick) cover it.
function Plate-Hover([double]$X, [double]$Y) {
  $at = Plate-At $X $Y
  [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0x04)
  Ui-Sync
}

# Expect "text" "what": as Ui-ExpectNew, and when it fails, first what the
# drawing logged of its hovers, picks and places since the mark.
function Expect([string]$Text, [string]$What) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if (@(Ui-NewLines | Where-Object { $_.Contains($Text) }).Count -gt 0) { Write-Host "ok   $What"; return }
    Start-Sleep -Milliseconds 100
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Write-Host "     waited for '$Text'; the drawing's lines since the mark:"
  foreach ($line in @(Ui-NewLines | Where-Object {
        $_ -match '^Drawing (hover|pick|picked|point|placing|preview|sheet view|view 1 at|command refused|annotations drawn)' })) {
    Write-Host "       $line"
  }
  Ui-Fail "${What}: '$Text' not in the log since the mark"
}

# Settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
function Settled([string]$Text) {
  for ($i = 0; $i -lt 20; $i++) {
    $before = Ui-LastLineWith "$Text at "
    Ui-Sync
    Start-Sleep -Milliseconds 300
    $after = Ui-LastLineWith "$Text at "
    if ($after -and $before -eq $after) { return }
  }
  Ui-Fail "the place of '$Text' did not settle"
}

# Drawing-Tool "name" "group": the tool from the ribbon's DRAWING tab, by
# its button when the tab shows it, else by its row in its group's menu,
# as tools/ui-windows-drawing-spacing-test.ps1 does.
function Drawing-Tool([string]$Name, [string]$Group) {
  Settled "Drawing group $Group"
  $lines = @(Ui-LogLines)
  $start = -1
  for ($i = $lines.Count - 1; $i -ge 0; $i--) {
    if ($lines[$i].Contains('Drawing area ')) { $start = $i; break }
  }
  $shown = $start -ge 0 -and @($lines | Select-Object -Skip $start | Where-Object { $_.Contains("Drawing tool $Name at ") }).Count -gt 0
  if ($shown) {
    $button = "Drawing tool $Name"
    Settled $button
    Ui-Step $Name                           { Ui-ClickLogged $button }
    return
  }
  $path = 'DRAWING/' + $Group.ToUpperInvariant()
  Ui-Step "the $Group menu (the group shows its name only)" { Ui-ClickLogged "Drawing group $Group" }
  Ui-ExpectNew "Ribbon menu ${path}: " "the $Group group's menu"
  $menu = "$(Ui-LastLineWith "Ribbon menu ${path}: ")"
  $entries = $menu.Substring($menu.IndexOf("Ribbon menu ${path}: ") + "Ribbon menu ${path}: ".Length)
  $rows = @($entries -split ' \| ' | Where-Object { $_ -ne '-' })
  $row = -1
  for ($i = 0; $i -lt $rows.Count; $i++) {
    if ($rows[$i] -eq $Name -or $rows[$i].StartsWith("$Name (")) { $row = $i; break }
  }
  if ($row -lt 0) { Ui-Fail "no $Name in the $Group group's menu: $entries" }
  Ui-Step "$Name (Down x$($row + 1), Enter)" { Ui-Key (@('Down') * ($row + 1)); Ui-Key 'Return' }
}

function No-Refusal {
  if (Ui-NewLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }
}

# Point-Of n: the sheet point the n-th click of a line or an arc went to
# (its last "Drawing point <n> at x,y" line), and whether it snapped.
function Point-Of([int]$N) {
  $line = Ui-LastLineWith "Drawing point $N at "
  if (-not $line) { Ui-Fail "no 'Drawing point $N' line in the log" }
  $match = [Text.RegularExpressions.Regex]::Match($line, "Drawing point $N at (-?[0-9.]+),(-?[0-9.]+)( \(snapped\))?")
  if (-not $match.Success) { Ui-Fail "an unreadable point: $line" }
  return @([double]::Parse($match.Groups[1].Value, $invariant), [double]::Parse($match.Groups[2].Value, $invariant),
           $match.Groups[3].Success)
}

# Apart a b mm what: two sheet points that far apart (to 0.3 mm: the
# points are logged to 0.1 mm), or fail.
function Apart($A, $B, [double]$Mm, [string]$What) {
  $d = [Math]::Sqrt(($A[0] - $B[0]) * ($A[0] - $B[0]) + ($A[1] - $B[1]) * ($A[1] - $B[1]))
  if ([Math]::Abs($d - $Mm) -gt 0.3) { Ui-Fail "${What}: $d mm apart, not $Mm" }
  Write-Host "ok   ${What}: $([Math]::Round($d, 2)) mm apart"
}

# Diameter-Drawn: annotation 1 drawn as the hole's diameter, its prefix
# (the diameter sign, matched as any letters: the log's encoding aside)
# before 16.
function Diameter-Drawn {
  $drawn = Ui-LastMatch 'Drawing annotations drawn: (.*)$'
  if (-not ($drawn -match '^1 = \D+16( \||$)')) { Ui-Fail "annotation 1 is not the hole's diameter: $drawn" }
  Write-Host "ok   drawn: the hole's diameter, $drawn"
}

Ui-StartApp 'drawing-dimension-picks' @('--open', $design)
Ui-ExpectLog 'plate.mitcad' 'the design opened'

Write-Host '--- The plate''s drawing'
Ui-Mark
Ui-Step 'open Drawing1 (double-click)'    { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
Expect 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
Expect 'Drawing sheet drawn: 1 views' 'its top view'
Expect 'Drawing sheet view ' 'the sheet placed'
No-Refusal

Write-Host '--- 1. One click on a hole: its diameter'
Ui-Mark
Drawing-Tool 'Dimension' 'Dimensions'
Expect 'Drawing tool: Dimension' 'the dimension tool is on, its Type Auto'
# The rims are clicked 45 degrees off the circles' seam vertex (at 0
# degrees), which a click there picks as a point.
Ui-Step 'the left hole''s rim'             { Plate-Click 35.657 19.343 }
Expect 'Drawing picked circle (1)' 'the hole picked'
Expect 'Drawing placing view' 'placed at once: one hole is its diameter'
Ui-Step 'point where it goes'             { Plate-Hover 14 44 }
Expect 'Drawing preview: leader diameter' 'the preview is a diameter'
Ui-Step 'where it goes'                   { Plate-Click 14 44 }
Expect '"type":"dimension"' 'a dimension asked for, its picks choosing its type'
Expect 'Drawing annotations drawn: 1 = ' 'drawn'
Diameter-Drawn
No-Refusal

Write-Host '--- 2. A hole and an edge placed between them: aligned'
Ui-Mark
Ui-Step 'the right hole''s rim'            { Plate-Click 56.343 37.657 }
Expect 'Drawing picked circle (1)' 'the hole picked'
Expect 'Drawing placing view' 'placed unless another pick comes'
Ui-Step 'the plate''s right edge'          { Plate-Click 90 12 }
Expect 'Drawing picked line (2)' 'the edge picked as the second pick'
Ui-Step 'point between them'              { Plate-Hover 78 28 }
Expect 'Drawing preview: dimension aligned' 'the preview is aligned'
Ui-Step 'where it goes, between them'     { Plate-Click 78 28 }
Expect ' | 2 = 28' 'drawn: 28 from the hole''s centre to the edge'
No-Refusal
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Write-Host '--- 3. Centerline on the slot''s sides: its centre arc'
Ui-Mark
Drawing-Tool 'Centerline' 'Geometry'
Expect 'Drawing tool: Centerline' 'the centerline tool is on'
Ui-Step 'the slot''s outer side'           { Plate-Click 56 14 }
Expect 'Drawing picked circle (1)' 'the outer side picked'
Ui-Step 'the slot''s inner side'           { Plate-Click 56 18 }
Expect '"type":"centerline"' 'a centerline asked for'
Expect ' | 2 = 28 | 3 = centerline' 'drawn: the slot''s centre arc'
No-Refusal
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Write-Host '--- 4. Arc from the slot''s centre between its ends'' centres'
Ui-Mark
Drawing-Tool 'Arc' 'Geometry'
Expect 'Drawing tool: Arc' 'the arc tool is on'
# Session 0's small window shows the sheet at about 0.44 pixels per sheet
# mm: a click lands within a model mm or so of where it is aimed, and the
# slot's ends' centres lie 2 mm from its sides' ends. The sheet is zoomed
# in about the view's middle (8 wheel steps, 1.15 each: about 3 times)
# and the clicks aim at the centres themselves.
$middle = Ui-LoggedAt 'Drawing view 1'
Ui-Mark
Ui-Step 'zoom in on the view (wheel)'     { [UiInput]::Wheel($script:UiWindow, $middle[0], $middle[1], 8); Ui-Sync }
Expect 'Drawing sheet view ' 'the sheet zoomed in'
Settled 'Drawing view 1'
Ui-Mark
Ui-Step 'its centre: the slot''s'          { Plate-Click 56 42 }
Expect 'Drawing point 1 at' 'the centre'
Ui-Step 'its start: the start''s centre'   { Plate-Click 45.0118 18.436 }
Expect 'Drawing point 2 at' 'the start'
Ui-Step 'its end: the end''s centre'       { Plate-Click 66.9882 18.436 }
Expect '"type":"arc"' 'an arc asked for'
Expect ' | 2 = 28 | 3 = centerline | 4 = arc' 'drawn'
No-Refusal
$center = Point-Of 1
$from = Point-Of 2
$to = Point-Of 3
foreach ($p in @($center, $from, $to)) {
  if (-not $p[2]) { Ui-Fail "a click of the arc did not snap: $($p[0]),$($p[1])" }
}
Write-Host 'ok   each click snapped'
# The slot's radius 26 and its ends 50 degrees apart, at 2:1.
Apart $center $from 52 'the slot''s centre and its start''s centre'
Apart $center $to 52 'the slot''s centre and its end''s centre'
Apart $from $to (104 * [Math]::Sin(25 * [Math]::PI / 180)) 'the centres of the slot''s ends'

Ui-Finish 'UI Windows drawing dimension picks'
