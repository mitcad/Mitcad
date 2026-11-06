# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/view/ViewController.cpp core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs tools/cli
# Input-driven Windows UI test of corners on drawings (mitcad#1338) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), the
# counterpart of tools/ui-drawing-corners-test.sh, on a plate that
# mitcad-cli makes: 90 x 50 x 5 with a hole of 16 through it at (30, 25);
# a drawing of it with its top view at 2:1 (its automatic centre marks
# on), so that the plate's point (x, y) lies at (120 + 2x, 100 + 2y) on the
# sheet:
#   1. Export PDF and SVG with Always show corners off: no corner dots.
#   2. Dimension: the plate's corner (90, 0), then the hole's vertical
#      centre line inside the hole, placed above them: a horizontal 60;
#      the centre line, then the corner, placed beside them: a vertical
#      25.
#   3. The app again with Always show corners on and Large in its
#      settings, the design as its file has it: the drawing's corners as
#      dots of 4.5 px; the same PDF and SVG exported, byte for byte (the
#      dots are on the screen only).
# The marker under the pointer is not waited for here: session 0 has no
# cursor, so Qt follows each posted move with a leave that clears the
# hover at once (tools/ui-windows-drawing-dimension-picks-test.ps1); the
# Linux test, the sheet's unit test (app/tests/drawing_sheet_view.cpp) and
# the model tests (drawing_pick) cover it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-corners-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-corners'),
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
        $_ -match '^Drawing (hover|pick|picked|placing|corner|sheet view|view 1 at|command refused|annotations drawn)' })) {
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

function Open-Drawing {
  Ui-Mark
  Ui-Step 'open Drawing1 (double-click)'  { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
  Expect 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
  Expect 'Drawing sheet drawn: 1 views' 'its top view'
  Expect 'Drawing sheet view ' 'the sheet placed'
}

# Export-As pdf|svg file: Export... of the open drawing to the file.
function Export-As([string]$Kind, [string]$File) {
  Ui-Mark
  Settled 'Drawing Export'
  Ui-Step 'Export...'                       { Ui-ClickLogged 'Drawing Export' }
  if ($Kind -eq 'pdf') {
    Ui-Step 'PDF... (Down, Enter)'          { Ui-Key 'Down'; Ui-Key 'Return' }
  } else {
    Ui-Step 'SVG... (Down x2, Enter)'       { Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Return' }
  }
  Ui-FocusDialog '^Export Drawing$'
  Ui-Step 'the path'                      { Ui-Key 'ctrl+a'; Ui-Type $File; Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew ': drawing Drawing1, 1 sheets' "the $Kind written" 30
  if (-not (Test-Path $File)) { Ui-Fail "no $File" }
}

function Same-Files([string]$A, [string]$B, [string]$What) {
  $a = [System.IO.File]::ReadAllBytes($A)
  $b = [System.IO.File]::ReadAllBytes($B)
  if ($a.Length -ne $b.Length) { Ui-Fail "${What}: $($a.Length) and $($b.Length) bytes" }
  for ($i = 0; $i -lt $a.Length; $i++) {
    if ($a[$i] -ne $b[$i]) { Ui-Fail "${What}: they differ at byte $i" }
  }
  Write-Host "ok   ${What}: the same"
}

Ui-StartApp 'drawing-corners' @('--open', $design)
Ui-ExpectLog 'plate.mitcad' 'the design opened'

Write-Host '--- The plate''s drawing, Always show corners off (the default)'
Open-Drawing
if (Ui-LogLines | Where-Object { $_ -match '^Drawing corner dots: [0-9]' }) { Ui-Fail 'corner dots while Always show corners is off' }
Write-Host 'ok   no corner dots'

Write-Host '--- 1. Exports without the dots (of the drawing as the file has it)'
$offPdf = Join-Path $work 'off.pdf'
$offSvg = Join-Path $work 'off.svg'
Export-As 'pdf' $offPdf
Export-As 'svg' $offSvg

Write-Host '--- 2. A corner to the hole''s centre line, in both orders'
Ui-Mark
Drawing-Tool 'Dimension' 'Dimensions'
Expect 'Drawing tool: Dimension' 'the dimension tool is on'
Ui-Step 'the plate''s corner'              { Plate-Click 90 0 }
Expect 'Drawing picked point (1)' 'the corner picked; a second pick waited for'
Ui-Step 'the hole''s centre line'          { Plate-Click 30 21 }
Expect 'Drawing picked center line (2)' 'the centre line picked as the second pick'
Expect 'Drawing placing view' 'the dimension placed next'
Ui-Step 'where it goes, above them'       { Plate-Click 60 40 }
Expect 'Drawing annotations drawn: 1 = 60' 'a horizontal 60 from the corner to the centre line'
No-Refusal
Ui-Mark
Ui-Step 'the hole''s centre line'          { Plate-Click 30 21 }
Expect 'Drawing picked center line (1)' 'the centre line picked first'
Ui-Step 'the plate''s corner'              { Plate-Click 90 0 }
Expect 'Drawing picked point (2)' 'the corner picked as the second pick'
Ui-Step 'where it goes, beside them'      { Plate-Click 100 10 }
Expect ' | 2 = 25' 'a vertical 25 from the centre line to the corner'
No-Refusal
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Expect 'Drawing tool: selection' 'back to the selection'

Write-Host '--- 3. Always show corners on, Large (the settings of a new start)'
# The design is not saved: the drawing opens again as the file has it, as
# it was exported above.
Ui-StopApp
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[drawing]', 'alwaysShowCorners=true', 'cornerDotSize=large')
Ui-StartApp 'drawing-corners-on' @('--open', $design)
Ui-ExpectLog 'plate.mitcad' 'the design opened again'
Open-Drawing
Expect 'Drawing corner dots: ' 'the corners as dots'
$line = Ui-LastLineWith 'Drawing corner dots: '
$match = [Text.RegularExpressions.Regex]::Match("$line", 'Drawing corner dots: ([0-9]+), radius 4\.5 px')
if (-not $match.Success -or [int]$match.Groups[1].Value -lt 4) { Ui-Fail "not the plate's corners as dots of 4.5 px: $line" }
Write-Host "ok   $line"
$onPdf = Join-Path $work 'on.pdf'
$onSvg = Join-Path $work 'on.svg'
Export-As 'pdf' $onPdf
Export-As 'svg' $onSvg
Same-Files $offPdf $onPdf 'the PDF with the corner dots on and off'
Same-Files $offSvg $onSvg 'the SVG with the corner dots on and off'

Ui-Finish 'UI Windows drawing corners'
