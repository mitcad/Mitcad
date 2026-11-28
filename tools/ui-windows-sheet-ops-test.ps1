# SPDX-License-Identifier: MIT
# Input-driven Windows UI test of the Sheet Metal operations (mitcad#125,
# after mitcad#210): each through its command panel with picks in the view,
# its preview, OK, the body's analytic volume (or the flat pattern's area),
# an edit from the timeline and Undo. Designs mitcad-cli makes (rules
# thickness t, bend radius r, K-factor K):
#   1. Fold of a 60 x 20 plate (t 2, r 3, K 0.3) along a sketch line: a bend
#      of angle a adds 20 a ((r + t)^2 - r^2) / 2 - 20 a (r + K t) t = 16 a,
#      so 2400 + 8 pi at 90 deg and 2400 + 4 pi at 45 deg. Esc of a Fold
#      leaves the plate as it is.
#   2. Bend at the seam of two coplanar faces of the same plate: 2400 + 8 pi,
#      2400 + 16 pi / 3 at 60 deg.
#   3. Corner Round and Corner Chamfer of a 40 x 30 plate's corners (t 2), in
#      the Floating layout: a round of radius R takes (R^2 - pi R^2 / 4) t, a
#      chamfer of distances d1, d2 takes d1 d2 t / 2.
#   4. On a 40 x 30 base (t 1, r 2, K 0.4) with a flange on its right edge
#      and one with offsets 3 on its back edge (the flanges' bend strips are
#      BA = 1.2 pi wide, h = BA / 2; a bend strip's material folded is
#      (r + t / 2) / (r + K t) = 25 / 24 of its developed area):
#      Flat Pattern (area 1617 + 73.2 pi), Corner Relief (the rules'
#      rectangle of 6 centred where the bends' centre lines cross: 6 (6 - h)
#      of the flat pattern, 2 (3 - h)(6 - h) + 25/24 2h (6 - h) of the
#      body), edited to a round relief of 2 inside the first bend's strip
#      (25/24 pi); Sheet Cut of a 3 x 9 rectangle across the back flange's
#      bend (19 mm2 flat and 4 mm2 of the strip); Unfold of the right bend
#      (its strip's 2h (24 + h) / 24 of excess), edited to both bends (the
#      volume is the flat pattern's area); Refold of all bends back to the
#      folded volume, edited to one bend.
#   5. Corner Relief of two flanges on different panels of a 60 x 20 base
#      (two coplanar faces, t 1, r 2, K 0.4; folded 1498 + 55 pi): the rules'
#      round relief of 2 lies in the right flange's bend strip, 25/24 pi.
# Mitcad runs with Mesa's software OpenGL (MITCAD_MESA_DIR) in session 0
# over SSH, as tools/ui-windows-sheet-test.ps1 does.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-ops-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-ops'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Plain F keys but F2 (Rename): no shortcut is another one without its modifier.
Ui-Init $App $Out $QtBin @{
  'sheet.bend' = 'F7'; 'sheet.fold' = 'F8'; 'sheet.cut' = 'Shift+F12'; 'sheet.corner_relief' = 'F11'
  'sheet.unfold' = 'F12'; 'sheet.refold' = 'F5'; 'sheet.corner_round' = 'Shift+F2'
  'sheet.corner_chamfer' = 'F3'; 'sheet.flat_pattern' = 'F4' } -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$workLogged = $work -replace '\\', '/'
$pi = [Math]::PI

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }

# The designs.
$rules2 = '{"cmd": "add_feature", "def": {"type": "sheet_rules", "thickness": 2, "bend_radius": 3, ' +
  '"allowance": {"type": "k_factor", "k": 0.3}, "reference_side": "back", ' +
  '"bend_relief": {"type": "rectangular", "width": 2, "depth": 3}, "corner_relief": {"type": "round", "width": 2, "depth": 3}}}'
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$designs = @{
  'fold' = @"
[
  $rules2,
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [-30, 0], "width": 60, "height": 20},
  {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F1", "profiles": [{"sketch": "F2", "region": "$rect"}]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F4", "start": [0, 0], "end": [0, 20]}
]
"@
  'bend' = @"
[
  $rules2,
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [-30, 0], "width": 60, "height": 20},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [0, 0], "end": [0, 20]},
  {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F1", "profiles": [
    {"sketch": "F2", "region": "r{c1[c4,c9],c3[c9,c4],c4[c3,c1],c9[c1,c3]}"},
    {"sketch": "F2", "region": "r{c1[c9,c2],c2[c1,c3],c3[c2,c9],c9[c1,c3]}"}]}}
]
"@
  'corner' = @"
[
  $rules2,
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F1", "profiles": [{"sketch": "F2", "region": "$rect"}]}}
]
"@
  'relief' = @"
[
  {"cmd": "add_feature", "def": {"type": "sheet_rules", "thickness": 1, "bend_radius": 2,
    "allowance": {"type": "k_factor", "k": 0.4}, "reference_side": "front",
    "bend_relief": {"type": "rectangular", "width": 1, "depth": 2}, "corner_relief": {"type": "rectangular", "width": 6, "depth": 2}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F1", "profiles": [{"sketch": "F2", "region": "$rect"}]}},
  {"cmd": "add_feature", "def": {"type": "sheet_flange", "body": "F3.b0",
    "boundary": {"panel": {"feature": "F3", "slot": 0}, "curve": {"type": "sketch", "sketch": "F2", "segment": "c2[c1,c3]"}},
    "angle": 1.5707963267948966, "length": {"mode": "tangent", "value": 10}, "position": "inside"}},
  {"cmd": "add_feature", "def": {"type": "sheet_flange", "body": "F3.b0",
    "boundary": {"panel": {"feature": "F3", "slot": 0}, "curve": {"type": "sketch", "sketch": "F2", "segment": "c3[c2,c4]"}},
    "angle": 1.5707963267948966, "length": {"mode": "tangent", "value": 10}, "position": "inside",
    "offsets": {"start": 3, "end": 3}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [32, 20], "width": 3, "height": 9}
]
"@
  'junction' = @"
[
  {"cmd": "add_feature", "def": {"type": "sheet_rules", "thickness": 1, "bend_radius": 2,
    "allowance": {"type": "k_factor", "k": 0.4}, "reference_side": "front",
    "bend_relief": {"type": "rectangular", "width": 1, "depth": 2}, "corner_relief": {"type": "round", "width": 2, "depth": 2}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [-30, 0], "width": 60, "height": 20},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [0, 0], "end": [0, 20]},
  {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F1", "profiles": [
    {"sketch": "F2", "region": "r{c1[c4,c9],c3[c9,c4],c4[c3,c1],c9[c1,c3]}"},
    {"sketch": "F2", "region": "r{c1[c9,c2],c2[c1,c3],c3[c2,c9],c9[c1,c3]}"}]}},
  {"cmd": "add_feature", "def": {"type": "sheet_flange", "body": "F3.b0",
    "boundary": {"panel": {"feature": "F3", "slot": 1}, "curve": {"type": "sketch", "sketch": "F2", "segment": "c2[c1,c3]"}},
    "angle": 1.5707963267948966, "length": {"mode": "tangent", "value": 10}, "position": "inside"}},
  {"cmd": "add_feature", "def": {"type": "sheet_flange", "body": "F3.b0",
    "boundary": {"panel": {"feature": "F3", "slot": 0}, "curve": {"type": "sketch", "sketch": "F2", "segment": "c3[c9,c4]"}},
    "angle": 1.5707963267948966, "length": {"mode": "tangent", "value": 10}, "position": "inside",
    "offsets": {"start": 3, "end": 3}}}
]
"@
}
foreach ($name in $designs.Keys) {
  $script = Join-Path $work "$name.json"
  Set-Content -Encoding ascii $script $designs[$name]
  $made = & $cli run $script --save (Join-Path $work "$name.mitcad")
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${name}: $($made -join ' / ')" }
}

# Ops-Pick "regex" [-Not name]: clicks where the view picks the first item
# (after the last "Pick places:") whose "Pick <kind> <owner>/<name>" matches,
# other than -Not; returns its name.
function Ops-Pick([string]$Pattern, [string]$Not = '') {
  $lines = Ui-LogLines
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  foreach ($line in $lines[$start..($lines.Count - 1)]) {
    if ($line -notmatch 'Pick \w+ (\S+) at (\d+),(\d+)$') { continue }
    $name = $Matches[1]
    $x = [int]$Matches[2]; $y = [int]$Matches[3]
    if ($name -match $Pattern -and $name -ne $Not) {
      Ui-Click $x $y
      return $name
    }
  }
  Ui-Fail "no pick place logged for /$Pattern/"
}

# Ops-PickAt "regex": the window point of the first item (after the last
# "Pick places:") whose name matches.
function Ops-PickAt([string]$Pattern) {
  $lines = Ui-LogLines
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  foreach ($line in $lines[$start..($lines.Count - 1)]) {
    if ($line -notmatch 'Pick \w+ (\S+) at (\d+),(\d+)$') { continue }
    $at = @([int]$Matches[2], [int]$Matches[3])
    if ($Matches[1] -match $Pattern) { return $at }
  }
  Ui-Fail "no pick place logged for /$Pattern/"
}

# Ops-ClickBetween "from regex" "to regex" t: a click at the fraction t of
# the way between two items' pick places (a place on a face that other
# items bound).
function Ops-ClickBetween([string]$From, [string]$To, [double]$T) {
  $a = Ops-PickAt $From
  $b = Ops-PickAt $To
  Ui-Click ([int]($a[0] + ($b[0] - $a[0]) * $T)) ([int]($a[1] + ($b[1] - $a[1]) * $T))
}

# Ops-ExpectVolume body expected description: the body's volume in a
# "Body ... volume a -> b mm3" line logged since the mark.
function Ops-ExpectVolume([string]$Body, [double]$Expected, [string]$Description) {
  $pattern = "Body \S+ \($([regex]::Escape($Body))\): volume [0-9.]+ -> ([0-9.]+) mm3"
  # Looked for every 50 ms (200 ms before, mitcad#870), up to 30 s.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $match = Ui-NewLines | Select-String -Pattern $pattern | Select-Object -Last 1
    if ($match) { break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 30000)
  if (-not $match) { Ui-Fail "${Description}: no volume of $Body logged since the mark" }
  $v = [double]::Parse($match.Matches[0].Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Abs($v - $Expected) -gt 0.01) { Ui-Fail "${Description}: volume $v mm3, expected $Expected" }
  Write-Host "ok   ${Description}: $v mm3 (expected $([Math]::Round($Expected, 4)))"
}

# Ops-ExpectArea expected description: the area in the "Flat pattern of
# Body1: valid, area <a> mm2" line logged since the mark (two decimals).
function Ops-ExpectArea([double]$Expected, [string]$Description) {
  $pattern = 'Flat pattern of Body1: valid, area ([0-9.]+) mm2'
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $match = Ui-NewLines | Select-String -Pattern $pattern | Select-Object -Last 1
    if ($match) { break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 30000)
  if (-not $match) { Ui-Fail "${Description}: no valid flat pattern logged since the mark" }
  $a = [double]::Parse($match.Matches[0].Groups[1].Value, [Globalization.CultureInfo]::InvariantCulture)
  if ([Math]::Abs($a - $Expected) -gt 0.006) { Ui-Fail "${Description}: area $a mm2, expected $Expected" }
  Write-Host "ok   ${Description}: $a mm2 (expected $([Math]::Round($Expected, 4)))"
}

function Ops-Undo([string]$Description) {
  Ui-Mark
  Ui-Step "undo (Ctrl+Z): $Description" { Ui-Key 'ctrl+z' }
  Ui-ExpectNew 'Undo:' "undone: $Description"
}

$panel = 'F3\.b0/F3:sheet_(front|back)'

Write-Host '=== Fold'
Ui-StartApp 'fold' @('--open', (Join-Path $work 'fold.mitcad'))
Ui-ExpectLog "Opened $workLogged/fold.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'fold (F8)'                         { Ui-Key 'F8' }
Ui-ExpectNew 'Command Fold started' 'Fold started'
Ui-Step 'cancel (Esc)'                      { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Fold cancelled' 'Fold cancelled'
Ui-Mark
Ui-Step 'fold (F8)'                         { Ui-Key 'F8' }
Ui-ExpectNew 'Pick places:' 'Fold logs where its picks are'
Ui-Step 'pick the plate'                    { Ops-Pick "^$panel\(0\)$" | Out-Null }
Ui-ExpectNew 'Fold Face: 1 face' 'the plate picked'
Ui-Step 'pick the line'                     { Ui-ClickLogged 'Sketch entity F4/c1' }
Ui-ExpectNew 'Fold Bend Line: 1 sketch curve' 'the line picked'
Ui-ExpectNew 'Preview Fold: ok' 'the fold previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added Fold as F5' 'the fold added'
Ui-ExpectNew 'Added fold, angle 90 deg, left side fixed' 'the fold described'
Ops-ExpectVolume 'F3.b0' (2400 + 8 * $pi) 'folded at 90 deg'
Ui-Mark
Ui-Step 'edit the fold (double-click)'      { Ui-DoubleClickLogged 'Timeline SheetFold1' }
Ui-ExpectNew 'Editing F5 with Fold' 'the fold opens for editing'
Ui-ExpectNew 'Fold Bend Line: 1 sketch curve' 'its line loaded'
Ui-Step 'angle 45'                          { Ui-TypeIn 'Panel Fold input angle' '45 deg' }
Ui-ExpectNew 'Preview Fold: ok' 'the 45 deg fold previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F5' 'the fold edited'
Ops-ExpectVolume 'F3.b0' (2400 + 4 * $pi) 'folded at 45 deg'
Ops-Undo 'the edit'
Ops-ExpectVolume 'F3.b0' (2400 + 8 * $pi) 'at 90 deg again'
Ops-Undo 'the fold'
Ops-ExpectVolume 'F3.b0' 2400 'flat again'

Write-Host '=== Bend'
Ui-StartApp 'bend' @('--open', (Join-Path $work 'bend.mitcad'))
Ui-ExpectLog "Opened $workLogged/bend.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'bend (F7)'                         { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'Bend logs where its picks are'
# The two faces' caps are one face of the body: the click's place tells the
# model which of them it is (sheet_reference "at"), a quarter of the way
# from the left or the right edge.
$leftEdge = 'E\{F3:sheet_(front|back)\(0\)\|F3:sheet_side0\(F2:sheet_curve\(c4\['
$rightEdge = 'E\{F3:sheet_(front|back)\(0\)\|F3:sheet_side1\(F2:sheet_curve\(c2\['
Ui-Step 'pick the left face'                { Ops-ClickBetween $leftEdge $rightEdge 0.25 }
Ui-ExpectNew 'Bend Fixed: 1 face' 'the fixed face picked'
Ui-Step 'pick the right face'               { Ops-ClickBetween $leftEdge $rightEdge 0.75 }
Ui-ExpectNew 'Bend Moving: 1 face' 'the moving face picked'
Ui-ExpectNew 'Preview Bend: ok' 'the bend previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added Bend as F4' 'the bend added'
Ops-ExpectVolume 'F3.b0' (2400 + 8 * $pi) 'bent at 90 deg'
Ui-Mark
Ui-Step 'edit the bend (double-click)'      { Ui-DoubleClickLogged 'Timeline SheetBend1' }
Ui-ExpectNew 'Editing F4 with Bend' 'the bend opens for editing'
Ui-ExpectNew 'Bend Moving: 1 face' 'its faces loaded'
Ui-Step 'angle 60'                          { Ui-TypeIn 'Panel Bend input angle' '60 deg' }
Ui-ExpectNew 'Preview Bend: ok' 'the 60 deg bend previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F4' 'the bend edited'
Ops-ExpectVolume 'F3.b0' (2400 + 16 * $pi / 3) 'bent at 60 deg'
Ops-Undo 'the edit'
Ops-ExpectVolume 'F3.b0' (2400 + 8 * $pi) 'at 90 deg again'
Ops-Undo 'the bend'
Ops-ExpectVolume 'F3.b0' 2400 'flat again'

Write-Host '=== Corner Round and Corner Chamfer (Floating layout)'
Ui-StartApp 'corner' @('--chrome=floating', '--open', (Join-Path $work 'corner.mitcad'))
Ui-ExpectLog "Opened $workLogged/corner.mitcad" 'the design opened floating'
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
$corner = 'E\{F3:sheet_side0\(F2:sheet_curve\([^)]*\)\)\|F3:sheet_side0\(F2:sheet_curve\([^)]*\)\)\}$'
Ui-Mark
Ui-Step 'corner round (Shift+F2)'           { Ui-Key 'shift+F2' }
Ui-ExpectNew 'Pick places:' 'Corner Round logs where its picks are'
$script:rounded = $null
Ui-Step 'pick a corner'                     { $script:rounded = Ops-Pick "F3\.b0/$corner" }
Ui-ExpectNew 'Corner Round Corners: 1 edge' 'the corner picked'
Ui-Step 'radius 2'                          { Ui-TypeIn 'Panel Corner Round input radius' '2' }
Ui-ExpectNew 'Preview Corner Round: ok' 'the round previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added corner round on 1 corner(s)' 'the round added'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (4 - $pi)) 'rounded with 2'
Ui-Mark
Ui-Step 'edit the round (double-click)'     { Ui-DoubleClickLogged 'Timeline SheetCornerRound1' }
Ui-ExpectNew 'with Corner Round' 'the round opens for editing'
Ui-ExpectNew 'Corner Round Corners: 1 edge' 'its corner loaded'
Ui-Step 'radius 3'                          { Ui-TypeIn 'Panel Corner Round input radius' '3' }
Ui-ExpectNew 'Preview Corner Round: ok' 'the larger round previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'the round edited'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (9 - 9 * $pi / 4)) 'rounded with 3'
Ops-Undo 'the edit'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (4 - $pi)) 'rounded with 2 again'
Ui-Mark
Ui-Step 'corner chamfer (F3)'               { Ui-Key 'F3' }
Ui-ExpectNew 'Pick places:' 'Corner Chamfer logs where its picks are'
Ui-Step 'pick another corner'               { Ops-Pick "F3\.b0/$corner" "F3.b0/$($script:rounded -replace '^F3\.b0/', '')" | Out-Null }
Ui-ExpectNew 'Corner Chamfer Corners: 1 edge' 'the corner picked'
Ui-Step 'distance 2'                        { Ui-TypeIn 'Panel Corner Chamfer input first' '2' }
Ui-Step 'distance 3'                        { Ui-TypeIn 'Panel Corner Chamfer input second' '3' }
Ui-ExpectNew 'Preview Corner Chamfer: ok' 'the chamfer previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added corner chamfer on 1 corner(s)' 'the chamfer added'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (4 - $pi) - 6) 'chamfered 2 x 3'
Ui-Mark
Ui-Step 'edit the chamfer (double-click)'   { Ui-DoubleClickLogged 'Timeline SheetCornerChamfer1' }
Ui-ExpectNew 'with Corner Chamfer' 'the chamfer opens for editing'
Ui-ExpectNew 'Corner Chamfer Corners: 1 edge' 'its corner loaded'
Ui-Step 'distance 4'                        { Ui-TypeIn 'Panel Corner Chamfer input first' '4' }
Ui-ExpectNew 'Preview Corner Chamfer: ok' 'the larger chamfer previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'the chamfer edited'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (4 - $pi) - 12) 'chamfered 4 x 3'
Ops-Undo 'the edit'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (4 - $pi) - 6) 'chamfered 2 x 3 again'
Ops-Undo 'the chamfer'
Ops-ExpectVolume 'F3.b0' (2400 - 2 * (4 - $pi)) 'only the round'

Write-Host '=== Corner Relief, Sheet Cut, Unfold, Refold'
$h = 0.6 * $pi
$folded = 1617 + 76.25 * $pi
$flat = 1617 + 73.2 * $pi
$relieved = $folded - 2 * (3 - $h) * (6 - $h) - 25 / 24 * 2 * $h * (6 - $h)
$cut = $relieved - 19 - 4 * 25 / 24
$flatCut = $flat - 6 * (6 - $h) - 23
Ui-StartApp 'relief' @('--open', (Join-Path $work 'relief.mitcad'))
Ui-ExpectLog "Opened $workLogged/relief.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'flat pattern (F4)'                 { Ui-Key 'F4' }
Ui-ExpectNew 'Pick places:' 'Flat Pattern logs where its picks are'
Ui-Step 'pick the base'                     { Ops-Pick "^$panel\(0\)$" | Out-Null }
Ui-ExpectNew 'Flat Pattern Fixed Face: 1 face' 'the base picked'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ops-ExpectArea $flat 'the flat pattern'

Write-Host '--- Corner Relief'
$bend1 = 'F3\.b0/F4:sheet_bend_(front|back)\(0\)$'
$bend2 = 'F3\.b0/F5:sheet_bend_(front|back)\(0\)$'
Ui-Mark
Ui-Step 'corner relief (F11)'               { Ui-Key 'F11' }
Ui-ExpectNew 'Pick places:' 'Corner Relief logs where its picks are'
Ui-Step 'pick the right bend'               { Ops-Pick $bend1 | Out-Null }
Ui-Step 'pick the back bend'                { Ops-Pick $bend2 | Out-Null }
Ui-ExpectNew 'Corner Relief Bends: 2 faces' 'both bends picked'
Ui-ExpectNew 'Preview Corner Relief: ok' 'the relief previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew "Added corner relief (the rules')" 'the relief added'
Ops-ExpectVolume 'F3.b0' $relieved 'the rules'' rectangle of 6'
Ui-Mark
Ui-Step 'edit the relief (double-click)'    { Ui-DoubleClickLogged 'Timeline SheetCornerRelief1' }
Ui-ExpectNew 'with Corner Relief' 'the relief opens for editing'
Ui-ExpectNew 'Corner Relief Bends: 2 faces' 'its bends loaded'
Ui-Step 'own corner relief'                 { Ui-ClickLogged 'Panel Corner Relief input custom_relief' }
Ui-ExpectNew 'Panel Corner Relief input relief_width at' 'the size shown'
Ui-Step 'size 2'                            { Ui-TypeIn 'Panel Corner Relief input relief_width' '2' }
Ui-ExpectNew 'Preview Corner Relief: ok' 'the round relief previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'the relief edited'
Ops-ExpectVolume 'F3.b0' ($folded - 25 / 24 * $pi) 'a round relief of 2 in the bend'
Ops-Undo 'the edit'
Ops-ExpectVolume 'F3.b0' $relieved 'the rectangle again'

Write-Host '--- Sheet Cut across the back bend'
Ui-Mark
Ui-Step 'sheet cut (Shift+F12)'                    { Ui-Key 'Shift+F12' }
Ui-ExpectNew 'Command Sheet Cut started' 'Sheet Cut started'
Ui-ExpectNew 'Sheet Cut Profiles: 1 profile' 'the newest profile taken'
Ui-Mark
Ui-Step 'the face input'                    { Ui-ClickLogged 'Panel Sheet Cut input stationary' }
Ui-ExpectNew 'Pick places:' 'the face input logs where its picks are'
Ui-Step 'pick the base'                     { Ops-Pick "^$panel\(0\)$" | Out-Null }
Ui-ExpectNew 'Sheet Cut Face: 1 face' 'the base picked'
Ui-Step 'pick the back bend'                { Ops-Pick $bend2 | Out-Null }
Ui-ExpectNew 'Sheet Cut Across Bends: 1 face' 'the bend picked'
Ui-ExpectNew 'Preview Sheet Cut: ok' 'the cut previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added sheet cut of 1 profile(s) across 1 bend(s)' 'the cut added'
Ops-ExpectVolume 'F3.b0' $cut 'cut across the bend'
Ui-Mark
Ui-Step 'edit the cut (double-click)'       { Ui-DoubleClickLogged 'Timeline SheetCut1' }
Ui-ExpectNew 'with Sheet Cut' 'the cut opens for editing'
Ui-ExpectNew 'Sheet Cut Across Bends: 1 face' 'its bend loaded'
Ui-ExpectNew 'Preview Sheet Cut: ok' 'the loaded cut previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'the unchanged cut edited'
Ui-Mark
Ui-Step 'flat pattern (F4)'                 { Ui-Key 'F4' }
Ui-ExpectNew 'Pick places:' 'Flat Pattern logs where its picks are'
Ui-Step 'pick the base'                     { Ops-Pick "^$panel\(0\)$" | Out-Null }
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ops-ExpectArea $flatCut 'the flat pattern with the relief and the cut'

Write-Host '--- Unfold and Refold'
Ui-Mark
Ui-Step 'unfold (F12)'                      { Ui-Key 'F12' }
Ui-ExpectNew 'Pick places:' 'Unfold logs where its picks are'
Ui-Step 'pick the base'                     { Ops-Pick "^$panel\(0\)$" | Out-Null }
Ui-ExpectNew 'Unfold Fixed Face: 1 face' 'the base picked'
Ui-Step 'pick the right bend'               { Ops-Pick $bend1 | Out-Null }
Ui-ExpectNew 'Unfold Bends: 1 face' 'the bend picked'
Ui-ExpectNew 'Preview Unfold: ok' 'the unfold previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added unfold of 1 bend(s)' 'the unfold added'
Ops-ExpectVolume 'F3.b0' ($cut - 2 * $h * (24 + $h) / 24) 'the right bend open'
Ui-Mark
Ui-Step 'edit the unfold (double-click)'    { Ui-DoubleClickLogged 'Timeline SheetUnfold1' }
Ui-ExpectNew 'with Unfold' 'the unfold opens for editing'
Ui-ExpectNew 'Unfold Bends: 1 face' 'its bend loaded'
Ui-Mark
Ui-Step 'the bends input'                   { Ui-ClickLogged 'Panel Unfold input bends' }
Ui-ExpectNew 'Pick places:' 'the bends input logs where its picks are'
Ui-Step 'pick the back bend'                { Ops-Pick $bend2 | Out-Null }
Ui-ExpectNew 'Unfold Bends: 2 faces' 'both bends'
Ui-ExpectNew 'Preview Unfold: ok' 'the whole unfold previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'the unfold edited'
Ops-ExpectVolume 'F3.b0' $flatCut 'flat: the flat pattern''s area'
Ui-Mark
Ui-Step 'refold (F5)'                       { Ui-Key 'F5' }
Ui-ExpectNew 'Command Refold started' 'Refold started'
Ui-ExpectNew 'Preview Refold: ok' 'the refold of all bends previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Added refold of SheetUnfold1 (all its bends)' 'the refold added'
Ops-ExpectVolume 'F3.b0' $cut 'folded again'
Ui-Mark
Ui-Step 'edit the refold (double-click)'    { Ui-DoubleClickLogged 'Timeline SheetRefold1' }
Ui-ExpectNew 'with Refold' 'the refold opens for editing'
Ui-Mark
Ui-Step 'the bends input'                   { Ui-ClickLogged 'Panel Refold input bends' }
Ui-ExpectNew 'Pick places:' 'the bends input logs where its picks are'
Ui-Step 'pick the back bend'                { Ops-Pick $bend2 | Out-Null }
Ui-ExpectNew 'Refold Bends: 1 face' 'the back bend picked'
Ui-ExpectNew 'Preview Refold: ok' 'the refold of one bend previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F' 'the refold edited'
Ops-ExpectVolume 'F3.b0' ($flatCut + (2 * $h * 31 - 4) / 24) 'only the back bend folded'
Ops-Undo 'the edit'
Ops-ExpectVolume 'F3.b0' $cut 'both folded again'
Ops-Undo 'the refold'
Ops-ExpectVolume 'F3.b0' $flatCut 'flat again'

Write-Host '=== Corner Relief of flanges on different panels'
Ui-StartApp 'junction' @('--open', (Join-Path $work 'junction.mitcad'))
Ui-ExpectLog "Opened $workLogged/junction.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'corner relief (F11)'               { Ui-Key 'F11' }
Ui-ExpectNew 'Pick places:' 'Corner Relief logs where its picks are'
Ui-Step 'pick the right bend'               { Ops-Pick $bend1 | Out-Null }
Ui-Step 'pick the back bend'                { Ops-Pick $bend2 | Out-Null }
Ui-ExpectNew 'Corner Relief Bends: 2 faces' 'both bends picked'
Ui-ExpectNew 'Preview Corner Relief: ok' 'the relief previews'
Ui-Step 'confirm (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew "Added corner relief (the rules')" 'the relief added'
Ops-ExpectVolume 'F3.b0' (1498 + 55 * $pi - 25 / 24 * $pi) 'the rules'' round relief of 2'
Ops-Undo 'the relief'
Ops-ExpectVolume 'F3.b0' (1498 + 55 * $pi) 'no relief'

Ui-Finish 'Windows sheet metal operations UI'
