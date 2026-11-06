# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs tools/cli
# Input-driven Windows UI test of a section's line and a break's sides
# snapping (mitcad#1362) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), the counterpart of
# tools/ui-drawing-section-snap-test.sh, on a plate that mitcad-cli makes:
# 90 x 50 x 5 with a hole of 16 through it at (30, 25); a drawing of it
# with its top view at 2:1 (its automatic centre marks on), so that the
# plate's point (x, y) lies at (120 + 2x, 100 + 2y) on the sheet and the
# hole's centre at (180, 150):
#   1. Section View: a click near the hole's centre snaps onto it, a
#      second on the hole's vertical centre line onto that line: a
#      vertical section line through the centre; placed beside the view,
#      a second view.
#   2. Section View: the centre, then a click about 46 degrees from it
#      near nothing: turned to 45 degrees; Esc.
#   3. Section View with Shift held for both clicks: both where clicked;
#      Esc.
#   4. Break: its first side near the hole's centre, its second near the
#      plate's top edge's middle (45, 50): both snapped, the break made.
# The marks under the pointer are not waited for here: session 0 has no
# cursor, so Qt follows each posted move with a leave that clears the
# hover at once (tools/ui-windows-drawing-dimension-picks-test.ps1); each
# click asks the model where it snaps when it is released, which this
# test checks.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-section-snap-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-section-snap'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

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
  {"expect": {"feature": "F4", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "2:1"}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Write-Host "ok   the plate made"

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

# Plate-At x y: the window point of a point of the plate (model mm) in its
# top view at 2:1, whose box (the plate, 90 x 50) has its middle at the
# logged "Drawing view 1 at x,y".
function Plate-At([double]$X, [double]$Y) {
  $middle = Ui-LastMatch 'Drawing view 1 at (-?[0-9]+,-?[0-9]+)'
  if (-not $middle) { Ui-Fail 'the app did not log where view 1 is' }
  $m = $middle -split ',' | ForEach-Object { [double]::Parse($_, $invariant) }
  $origin = Sheet-At 0 0
  $unit = Sheet-At 100 100
  $sx = ($unit[0] - $origin[0]) / 100.0
  $sy = ($unit[1] - $origin[1]) / 100.0
  $px = [int][Math]::Round($m[0] + $sx * 2 * ($X - 45))
  $py = [int][Math]::Round($m[1] + $sy * 2 * ($Y - 25))
  Write-Host "     plate $X,$Y -> window $px,$py (view 1's middle at $middle)"
  return @($px, $py)
}

function Plate-Click([double]$X, [double]$Y) {
  $at = Plate-At $X $Y
  Ui-Click $at[0] $at[1]
  Ui-Sync
}

function Plate-ShiftClick([double]$X, [double]$Y) {
  $at = Plate-At $X $Y
  Ui-ClickWith 'shift' $at[0] $at[1]
  Ui-Sync
}

function Sheet-Click([double]$X, [double]$Y) {
  $at = Sheet-At $X $Y
  Ui-Click $at[0] $at[1]
  Ui-Sync
}

# Expect "text" "what": as Ui-ExpectNew, and when it fails, first what the
# drawing logged of its points and places since the mark.
function Expect([string]$Text, [string]$What) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if (@(Ui-NewLines | Where-Object { $_.Contains($Text) }).Count -gt 0) { Write-Host "ok   $What"; return }
    Start-Sleep -Milliseconds 100
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Write-Host "     waited for '$Text'; the drawing's lines since the mark:"
  foreach ($line in @(Ui-NewLines | Where-Object {
        $_ -match '^Drawing (point|snap|placing|sheet view|view 1 at|command refused|sheet drawn|tool)' })) {
    Write-Host "       $line"
  }
  Ui-Fail "${What}: '$Text' not in the log since the mark"
}

# Point-Line n: the newest "Drawing point <n> at ..." line since the mark.
function Point-Line([int]$N) {
  $lines = @(Ui-NewLines | Where-Object { $_.Contains("Drawing point $N at ") })
  if ($lines.Count -eq 0) { Ui-Fail "no point $N since the mark" }
  $line = $lines[$lines.Count - 1]
  return $line.Substring($line.IndexOf("Drawing point $N at "))
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
# as tools/ui-windows-drawing-dimension-picks-test.ps1 does.
function Drawing-Tool([string]$Name, [string]$Group) {
  Ui-Mark
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
  } else {
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
  Expect "Drawing tool: $Name" "the $Name tool is on"
}

function No-Refusal {
  if (Ui-NewLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }
}

Ui-StartApp 'drawing-section-snap' @('--open', $design)
Ui-ExpectLog 'plate.mitcad' 'the design opened'

Ui-Mark
Ui-Step 'open Drawing1 (double-click)'    { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
Expect 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
Expect 'Drawing sheet drawn: 1 views' 'its top view'
Expect 'Drawing sheet view ' 'the sheet placed'

Write-Host '--- 1. A vertical section line through the hole''s centre'
Drawing-Tool 'Section View' 'Create'
Ui-Mark
Ui-Step 'near the hole''s centre'          { Plate-Click 30.3 25.2 }
Expect 'Drawing point snapped to ' 'the click snapped'
Expect 'Drawing point 1 at 180.0,150.0 (snapped)' 'onto the hole''s centre'
Ui-Step 'on its vertical centre line'     { Plate-Click 30.3 33 }
Expect 'Drawing point 2 at 180.0,' 'onto the centre line: a vertical line'
$line = Point-Line 2
if (-not $line.EndsWith('(snapped)')) { Ui-Fail "the second point did not snap: $line" }
Write-Host "ok   $line"
Expect 'Drawing placing view' 'the section placed next'
Ui-Step 'where it goes, beside the view'  { Sheet-Click 360 150 }
Expect 'Drawing sheet drawn: 2 views' 'the section view made'
No-Refusal

Write-Host '--- 2. Near 45 degrees: 45 degrees'
Drawing-Tool 'Section View' 'Create'
Ui-Mark
Ui-Step 'near the hole''s centre'          { Plate-Click 30.3 25.2 }
Expect 'Drawing point 1 at 180.0,150.0 (snapped)' 'onto the hole''s centre'
Ui-Step 'about 46 degrees from it'        { Plate-Click 50 46 }
Expect 'Drawing snap: section line at 45 degrees' 'turned to 45 degrees'
$line = Point-Line 2
$match = [Text.RegularExpressions.Regex]::Match($line, 'at ([-0-9.]+),([-0-9.]+) \(at 45 degrees\)$')
if (-not $match.Success) { Ui-Fail "the second point not at 45 degrees: $line" }
$dx = [double]::Parse($match.Groups[1].Value, $invariant) - 180
$dy = [double]::Parse($match.Groups[2].Value, $invariant) - 150
if ($dx -le 0 -or [Math]::Abs($dx - $dy) -gt 0.2) { Ui-Fail "the second point is not on the 45 degree line: $line" }
Write-Host "ok   $line"
Ui-Mark
Ui-Step 'drop it (Esc)'                   { Ui-Key 'Escape' }

Write-Host '--- 3. Shift held: where it is clicked'
Ui-Mark
Ui-Step 'near the centre, Shift held'     { Plate-ShiftClick 30.75 25.5 }
Ui-Step 'near upright, Shift held'        { Plate-ShiftClick 31 60 }
Expect 'Drawing point 2 at ' 'both clicks taken'
if (Ui-NewLines | Where-Object { $_.Contains('Drawing point snapped to ') -or $_.Contains('Drawing snap: section line') }) {
  Ui-Fail 'a click with Shift snapped'
}
foreach ($n in 1, 2) {
  $line = Point-Line $n
  if ($line.Contains('(snapped)') -or $line.Contains('degrees)') -or $line.Contains(' at 180.0,')) {
    Ui-Fail "click $n with Shift moved: $line"
  }
  Write-Host "ok   $line"
}
Ui-Mark
Ui-Step 'drop it (Esc)'                   { Ui-Key 'Escape' }
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Write-Host '--- 4. A break''s sides snap the same way'
Drawing-Tool 'Break' 'Create'
Ui-Mark
Ui-Step 'near the hole''s centre'          { Plate-Click 30.3 25.2 }
Expect 'Drawing point 1 at 180.0,150.0 (snapped)' 'onto the hole''s centre'
Ui-Step 'near the top edge''s middle'     { Plate-Click 45.3 49.7 }
Expect 'Drawing point 2 at 210.0,200.0 (snapped)' 'onto the edge''s middle'
Expect '"add_break"' 'the break asked for'
No-Refusal

Ui-Finish 'UI Windows drawing section snap'
