# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs core/model/src/drawing tools/cli
# Input-driven Windows UI test of a flange's bolt circle and the angle
# between its holes (mitcad#1693) in the build VM (tools/ui-windows-lib.ps1
# says how the input is posted), the counterpart of
# tools/ui-drawing-bolt-circle-test.sh, on a plate that mitcad-cli makes:
# 120 x 120 x 5 with six holes of 10 through it on a circle of 60 about
# (60, 60), from 0 degrees every 60; a drawing of it with its top view at
# 1:1:
#   1. Center Pattern: one hole's rim; every hole of the pattern marked.
#   2. Dimension, Auto: a click on the pattern's circle between two holes
#      picks that circle ("pattern_circle"), placed: a diameter of 60, B.C.
#   3. Dimension, Angle: the radial lines of the holes at 60 and 120
#      degrees, outside their rims, picked as centre lines
#      ("pattern_line"), placed between them: 60 degrees.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-bolt-circle-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-bolt-circle'),
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
$design = Join-Path $work 'flange.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 120, "height": 120},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [90, 60], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [75, 85.98076211353316], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [45, 85.98076211353316], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 60], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [45, 34.01923788646684], "diameter": 10},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [75, 34.01923788646684], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [
      {"sketch": "F3", "region": "r{c1}"}, {"sketch": "F3", "region": "r{c3}"},
      {"sketch": "F3", "region": "r{c5}"}, {"sketch": "F3", "region": "r{c7}"},
      {"sketch": "F3", "region": "r{c9}"}, {"sketch": "F3", "region": "r{c11}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "1:1"}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Write-Host "ok   the flange made"

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

# Polar-At degrees radius: the window point of the plate's point at an
# angle and a distance from the pattern's centre (60, 60), in its top view
# at 1:1, whose box (the plate, 120 x 120) has its middle (the pattern's
# centre) at the logged "Drawing view 1 at x,y".
function Polar-At([double]$Degrees, [double]$Radius) {
  $middle = Ui-LastMatch 'Drawing view 1 at (-?[0-9]+,-?[0-9]+)'
  if (-not $middle) { Ui-Fail 'the app did not log where view 1 is' }
  $m = $middle -split ',' | ForEach-Object { [double]::Parse($_, $invariant) }
  $origin = Sheet-At 0 0
  $unit = Sheet-At 100 100
  $sx = ($unit[0] - $origin[0]) / 100.0
  $sy = ($unit[1] - $origin[1]) / 100.0
  $a = $Degrees * [Math]::PI / 180
  $px = [int][Math]::Round($m[0] + $sx * $Radius * [Math]::Cos($a))
  $py = [int][Math]::Round($m[1] + $sy * $Radius * [Math]::Sin($a))
  Write-Host "     $Degrees degrees, $Radius from the centre -> window $px,$py (view 1's middle at $middle)"
  return @($px, $py)
}

function Polar-Click([double]$Degrees, [double]$Radius) {
  $at = Polar-At $Degrees $Radius
  Ui-Click $at[0] $at[1]
  Ui-Sync
}

# Expect "text" "what": as Ui-ExpectNew, and when it fails, first what the
# drawing logged of its picks and places since the mark.
function Expect([string]$Text, [string]$What) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if (@(Ui-NewLines | Where-Object { $_.Contains($Text) }).Count -gt 0) { Write-Host "ok   $What"; return }
    Start-Sleep -Milliseconds 100
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Write-Host "     waited for '$Text'; the drawing's lines since the mark:"
  foreach ($line in @(Ui-NewLines | Where-Object {
        $_ -match '^Drawing (hover|pick|picked|placing|option|sheet view|view 1 at|command refused|annotations drawn)' })) {
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
# as tools/ui-windows-drawing-corners-test.ps1 does.
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

Ui-StartApp 'drawing-bolt-circle' @('--open', $design)
Ui-ExpectLog 'flange.mitcad' 'the design opened'

Write-Host '--- The flange''s drawing'
Ui-Mark
Ui-Step 'open Drawing1 (double-click)'    { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
Expect 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
Expect 'Drawing sheet drawn: 1 views' 'its top view'
Expect 'Drawing sheet view ' 'the sheet placed'

Write-Host '--- 1. A centre pattern on the holes'
Ui-Mark
Drawing-Tool 'Center Pattern' 'Geometry'
Expect 'Drawing tool: Center Pattern' 'the tool is on'
Ui-Step 'the rim of the hole at 0 degrees' { Polar-Click 0 35 }
Expect '"type":"center_pattern"' 'a centre pattern asked for'
Expect '1 = center_pattern' 'drawn'
No-Refusal
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Write-Host '--- 2. The bolt circle''s diameter (Auto)'
Ui-Mark
Drawing-Tool 'Dimension' 'Dimensions'
Expect 'Drawing tool: Dimension' 'the dimension tool is on'
Ui-Step 'the pattern''s circle at 30 degrees' { Polar-Click 30 30 }
Expect 'Drawing picked circle (1)' 'a circle picked'
Expect '"kind":"pattern_circle"' 'the centre pattern''s circle'
Expect 'Drawing placing view' 'its place is clicked next'
Ui-Step 'where it goes, up right'         { Polar-Click 45 70 }
Expect '"type":"dimension"' 'a dimension asked for'
Expect '60 B.C.' 'a diameter of 60, the bolt circle''s'
No-Refusal

Write-Host '--- 3. The angle between two radial lines (Angle)'
Ui-Mark
Settled 'Drawing panel drawingOption_type'
Ui-Step 'Type: Angle (Home, Down x3, Enter)' {
  Ui-ClickLogged 'Drawing panel drawingOption_type'
  Ui-Key 'Home'; Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Return'
}
Expect 'Drawing option type: angle' 'an angle next'
Ui-Step 'the radial line at 60 degrees'   { Polar-Click 60 35.6 }
Expect 'Drawing picked center line (1)' 'a centre line picked'
Expect '"kind":"pattern_line"' 'the pattern''s radial line'
Ui-Step 'the radial line at 120 degrees'  { Polar-Click 120 35.6 }
Expect 'Drawing picked center line (2)' 'the second centre line picked'
Expect 'Drawing placing view' 'its place is clicked next'
Ui-Step 'where it goes, between them'     { Polar-Click 90 45 }
Expect '"type":"angle"' 'an angle asked for'
Expect ' | 3 = 60' '60 degrees between the holes'
No-Refusal
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Ui-Finish 'UI Windows drawing bolt circle'
