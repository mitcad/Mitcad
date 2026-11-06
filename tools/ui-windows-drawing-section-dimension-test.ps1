# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs core/model/src/drawing/annotate_def.rs tools/cli
# Input-driven Windows UI test of a section's cut edge dimensioned alone
# (mitcad#1361) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), the counterpart of
# tools/ui-drawing-section-dimension-test.sh, on a part that mitcad-cli
# makes: a base 64 x 30 x 10 with a boss 32 wide and 20 high in its middle
# (x 16..48) and a hole of 4 through both at (32, 15); a drawing with its
# top view at 2:1 and a section through the hole along x (view 2, at 2:1
# too), so that the bottom's cut is two pieces, x 0..30 and 34..64:
#   1. Dimension: the bottom's cut edge beside the hole picked alone is
#      placed at once and reads 30, the piece under the click, not 64
#      across the hole.
#   2. The hole's corner (30, 0) waits for a second pick and the hint says
#      so; the part's far corner (64, 0) then, placed below them: 34.
# The part is symmetric about x 32, so the clicks hold whichever way the
# section looks. The hint under the pointer before a click is not waited
# for here: session 0 has no cursor, so Qt follows each posted move with a
# leave that clears the hover at once
# (tools/ui-windows-drawing-dimension-picks-test.ps1); the Linux test and
# the model tests (drawing_pick's `alone`) cover it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-section-dimension-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-section-dimension'),
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
$design = Join-Path $work 'stepped.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 64, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [16, 0], "width": 32, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "join"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [32, 15], "diameter": 4},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F6", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [150, 200], "scale": "2:1"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
    "line": [[76, 200], [224, 200]], "flip": true, "at": [150, 100]}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Write-Host "ok   the part made"

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

# Section-At x z: the window point of the part's point (x, z) in the
# section at 2:1, whose box (the part's cut, 64 x 20) has its middle at the
# logged "Drawing view 2 at x,y".
function Section-At([double]$X, [double]$Z) {
  $middle = Ui-LastMatch 'Drawing view 2 at (-?[0-9]+,-?[0-9]+)'
  if (-not $middle) { Ui-Fail 'the app did not log where the section is' }
  $m = $middle -split ',' | ForEach-Object { [double]::Parse($_, $invariant) }
  $origin = Sheet-At 0 0
  $unit = Sheet-At 100 100
  $sx = ($unit[0] - $origin[0]) / 100.0
  $sy = ($unit[1] - $origin[1]) / 100.0
  $px = [int][Math]::Round($m[0] + $sx * 2 * ($X - 32))
  $py = [int][Math]::Round($m[1] + $sy * 2 * ($Z - 10))
  Write-Host "     section $X,$Z -> window $px,$py (view 2's middle at $middle)"
  return @($px, $py)
}

function Section-Click([double]$X, [double]$Z) {
  $at = Section-At $X $Z
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
        $_ -match '^Drawing (hover|hint|pick|picked|placing|sheet view|view 2 at|command refused|annotations drawn)' })) {
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
# as tools/ui-windows-drawing-dimension-picks-test.ps1 does.
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

Ui-StartApp 'drawing-section-dimension' @('--open', $design)
Ui-ExpectLog 'stepped.mitcad' 'the design opened'

Write-Host '--- The part''s drawing: its top view and the section'
Ui-Mark
Ui-Step 'open Drawing1 (double-click)'    { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
Expect 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
Expect 'Drawing sheet drawn: 2 views' 'its top view and the section'
Expect 'Drawing view 2 at ' 'the section placed'
Settled 'Drawing view 2'

Write-Host '--- 1. The cut edge beside the hole, alone'
Ui-Mark
Drawing-Tool 'Dimension' 'Dimensions'
Expect 'Drawing tool: Dimension' 'the dimension tool is on'
Ui-Step 'the bottom''s cut edge'            { Section-Click 15 0 }
Expect 'Drawing picked line (1)' 'the cut edge picked'
Expect '"on_cut":' 'the piece under the click'
Expect 'Drawing placing view' 'placed at once, alone'
Ui-Step 'where it goes, below it'         { Section-Click 15 -8 }
Expect 'Drawing annotations drawn: 1 = 30' 'the piece''s length, 30, not 64 across the hole'
No-Refusal

Write-Host '--- 2. A corner waits for a second pick'
Ui-Mark
Ui-Step 'the hole''s corner'               { Section-Click 30 0 }
Expect 'Drawing picked point (1)' 'the corner picked'
Expect 'Drawing hint: Click a second edge, circle or corner to dimension it from' 'a second pick waited for'
if (Ui-NewLines | Where-Object { $_.Contains('Drawing placing view') }) { Ui-Fail 'a corner alone was placed' }
Write-Host 'ok   not placed alone'
Ui-Step 'the part''s far corner'           { Section-Click 64 0 }
Expect 'Drawing picked point (2)' 'the second corner picked'
Expect 'Drawing placing view' 'the dimension placed next'
Ui-Step 'where it goes, below them'       { Section-Click 47 -16 }
Expect ' | 2 = 34' '34 from the hole''s corner to the far one'
No-Refusal
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Ui-Finish 'UI Windows drawing section dimension'
