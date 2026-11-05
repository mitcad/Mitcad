# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewTessellation.hpp
# check-all sources: app/framework/DisplayMeshes.cpp app/framework/DisplayMeshes.hpp geometry/io
# check-all sources: core/model/src/features/pattern.rs app/commands/PatternCommands.cpp
# check-all sources: app/commands/MoveCommands.cpp app/framework/CommandSession.cpp tools/cli
# Input-driven Windows UI test of a pattern of bodies and of window
# selection over many bodies (mitcad#1275) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), on a design
# mitcad-cli makes: a 10 mm cube with its corner at (20, 20, 0) and a 4 mm
# hole down through its middle (Body1, F2.b0). Results, not times:
#   1. A circular pattern of the cube about Z, 12 in a full turn, made in
#      the UI. Its copies are the cube moved as a location, so the
#      command's display meshing meshes no copy: at most the cube's own
#      faces, once (MITCAD_MESH_TRACE: "Mesh trace ... final: <ready>
#      bodies ready, <meshed> meshed, ...; faces=... meshed=<faces>").
#   2. Move/Copy, a window dragged to the right round all 12 bodies (the
#      view zoomed out until their places fit in a quarter of it): every
#      body decided by its box ("Window pick by bounds: 12 inside, 0
#      outside, 0 crossing"), all 12 taken.
#   3. Move/Copy again, a window dragged to the left (crossing) a few
#      pixels round the cube's place: the cube's box crosses it, so its
#      triangles decide, and it is taken; the bodies far from it are out by
#      their boxes alone.
#   4. Move/Copy again, a window dragged to the right in the empty middle
#      of the ring: every body out by its box, none taken.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-window-bodies-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-window-bodies'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.circular_pattern' = 'F7'; 'solid.move' = 'F8' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'window-bodies.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, 20], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [25, 25], "diameter": 4},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "start": {"type": "offset", "offset": -5}, "extent": {"type": "distance", "distance": 30},
    "operation": "cut"}},
  {"expect": {"bodies": 1}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Body-Places: each shown body's last logged place ("Body <uid> at x,y",
# its box's middle in the window), uid -> @(x, y).
function Body-Places {
  $places = @{}
  foreach ($line in Ui-LogLines) {
    $m = [regex]::Match($line, '^Body (\S+) at (-?\d+),(-?\d+)$')
    if ($m.Success) { $places[$m.Groups[1].Value] = @([int]$m.Groups[2].Value, [int]$m.Groups[3].Value) }
  }
  return $places
}

# Places-Box places: the rectangle the places span (Left, Top, Right, Bottom).
function Places-Box($Places) {
  $box = @{ Left = [int]::MaxValue; Top = [int]::MaxValue; Right = [int]::MinValue; Bottom = [int]::MinValue }
  foreach ($at in $Places.Values) {
    $box.Left = [Math]::Min($box.Left, $at[0]); $box.Right = [Math]::Max($box.Right, $at[0])
    $box.Top = [Math]::Min($box.Top, $at[1]); $box.Bottom = [Math]::Max($box.Bottom, $at[1])
  }
  return $box
}

# Expect-Bounds "<i> inside, <o> outside, <c> crossing" "description": the
# window's decision by the bodies' boxes, logged since the mark.
function Expect-Bounds([string]$Counts, [string]$Description) {
  Ui-ExpectNew "Window pick by bounds: $Counts" $Description
}

# Last-Bounds: the counts of the last "Window pick by bounds" line since
# the mark, @(inside, outside, crossing).
function Last-Bounds {
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-NewLines | Where-Object { $_.StartsWith('Window pick by bounds: ') } | Select-Object -Last 1
    $m = [regex]::Match("$line", ': (\d+) inside, (\d+) outside, (\d+) crossing$')
    if ($m.Success) { return @([int]$m.Groups[1].Value, [int]$m.Groups[2].Value, [int]$m.Groups[3].Value) }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail 'the window was not decided by the bodies'' boxes (no "Window pick by bounds" line)'
}

# Window-Drag x1 y1 x2 y2: a window selection from the first point to the
# second (to the left: what it crosses), its release handled.
function Window-Drag([int]$X1, [int]$Y1, [int]$X2, [int]$Y2) {
  Ui-Drag $X1 $Y1 $X2 $Y2
}

# The meshes of the view say what they meshed (Mesh trace lines).
$env:MITCAD_MESH_TRACE = '1'
Ui-StartApp 'window-bodies' @('--open', $design)
Ui-ExpectLog 'window-bodies.mitcad' 'the design opened'
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cube shown'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }

Write-Host '--- 1. A circular pattern of the cube: 12 about Z'
Ui-Mark
Ui-Step 'circular pattern (F7)'          { Ui-Key 'F7' }
Ui-ExpectNew 'Command Circular Pattern started' 'the panel open'
Ui-Step 'pick Body1'                     { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Circular Pattern Objects: 1 body [body F2.b0]' 'the cube picked'
Ui-Step 'the axis input'                 { Ui-ClickLogged 'Panel Circular Pattern input axis' }
Ui-PickOriginRow 'Z'
Ui-ExpectNew 'Circular Pattern Axis: 1 axis [axis z]' 'Z picked'
Ui-Step 'quantity 12'                    { Ui-TypeIn 'Panel Circular Pattern input quantity' '12' }
Ui-ExpectNew 'Preview Circular Pattern: ok' 'previewed' 30
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added Circular Pattern as ' 'the pattern added' 30
Ui-WaitIdle
Ui-Sync
# (Logged when the bodies shown change: the preview may have shown them.)
$shown = "$(Ui-LastMatch 'Bodies shown: (.*)$')" -split ', '
if ($shown.Count -ne 12) { Ui-Fail "12 bodies shown, not $($shown.Count): $($shown -join ', ')" }
Write-Host 'ok   12 bodies shown'
$trace = Ui-NewLines | Where-Object { $_ -match '^Mesh trace .* final: ' } | Select-Object -Last 1
$m = [regex]::Match("$trace", ' final: (\d+) bodies ready, (\d+) meshed, .*; faces=\d+ kept=\d+ meshed=(\d+) ')
if (-not $m.Success) { Ui-Fail "no final display meshing traced for the pattern ($trace)" }
$ready = [int]$m.Groups[1].Value
$bodies = [int]$m.Groups[2].Value
$faces = [int]$m.Groups[3].Value
# The cube with its hole has 7 faces (or 8 with the hole's wall in two).
if ($ready + $bodies -ne 12 -or $faces -gt 8) {
  Ui-Fail "the pattern's copies meshed by themselves: $ready ready, $bodies meshed, $faces faces ($trace)"
}
Write-Host "ok   the copies drawn from the cube's meshes: $ready bodies ready, $bodies meshed, $faces faces meshed"
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- 2. A window round all 12 bodies'
Ui-Mark
Ui-Step 'move/copy (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Command Move/Copy started' 'Move/Copy started'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
# Zoomed out about the bodies until their places span a quarter of the
# view at most: the window round them, with room for the bodies beyond
# their middles, then lies in the view's free part (beside the Browser card
# and the panel).
$area = (Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)') -split ' ' | ForEach-Object { [int]$_ }
$limit = [Math]::Min($area[2], $area[3]) * 0.25
for ($i = 0; $i -le 16; $i++) {
  $places = Body-Places
  if ($places.Count -ne 12) { Ui-Fail "the places of 12 bodies not logged ($($places.Count))" }
  $box = Places-Box $places
  if ($box.Right - $box.Left -le $limit -and $box.Bottom - $box.Top -le $limit) { break }
  if ($i -eq 16) { Ui-Fail "the bodies' places still span $($box.Right - $box.Left) x $($box.Bottom - $box.Top) px" }
  $cx = [int](($box.Left + $box.Right) / 2)
  $cy = [int](($box.Top + $box.Bottom) / 2)
  $before = Ui-CountLog '(?m)^Body \S+ at '
  [UiInput]::Move($script:UiWindow, $cx, $cy, 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Wheel($script:UiWindow, $cx, $cy, -1)
  for ($j = 0; $j -lt 20; $j++) {
    Start-Sleep -Milliseconds 200
    if ((Ui-CountLog '(?m)^Body \S+ at ') -ge $before + 12) { break }
  }
  Ui-Sync
}
$box = Places-Box (Body-Places)
$w = $box.Right - $box.Left
$h = $box.Bottom - $box.Top
$margin = [int](30 + 0.25 * [Math]::Max($w, $h))
Write-Host "ok   the bodies' places span $w x $h px; the window $margin px round them"
Ui-Mark
Ui-Step 'a window round them (to the right)' {
  Window-Drag ($box.Left - $margin) ($box.Top - $margin) ($box.Right + $margin) ($box.Bottom + $margin)
}
Expect-Bounds '12 inside, 0 outside, 0 crossing' 'every body decided by its box, inside'
Ui-ExpectNew 'Move/Copy Objects: 12 bodies [' 'all 12 bodies taken'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Move/Copy cancelled' 'cancelled'

Write-Host '--- 3. A crossing window at the cube'
Ui-Mark
Ui-Step 'move/copy (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Command Move/Copy started' 'Move/Copy started'
$cube = (Body-Places)['F2.b0']
if (-not $cube) { Ui-Fail 'no place of the cube logged' }
Ui-Mark
Ui-Step 'a window across the cube (to the left)' {
  Window-Drag ($cube[0] + 6) ($cube[1] + 6) ($cube[0] - 6) ($cube[1] - 6)
}
$counts = Last-Bounds
if ($counts[0] -ne 0 -or $counts[2] -lt 1 -or $counts[1] -lt 1 -or $counts[0] + $counts[1] + $counts[2] -ne 12) {
  Ui-Fail "the crossing window's decision: $($counts[0]) inside, $($counts[1]) outside, $($counts[2]) crossing"
}
Write-Host "ok   $($counts[2]) crossing by their boxes (the cube's among them), $($counts[1]) out"
Ui-ExpectNew 'Move/Copy Objects: ' 'bodies taken'
$taken = "$(Ui-NewLines | Where-Object { $_.StartsWith('Move/Copy Objects: ') } | Select-Object -Last 1)"
if (-not $taken.Contains('body F2.b0')) { Ui-Fail "the cube not taken: $taken" }
Write-Host "ok   the cube taken by its triangles ($taken)"
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Move/Copy cancelled' 'cancelled'

Write-Host '--- 4. A window in the empty middle of the ring'
Ui-Mark
Ui-Step 'move/copy (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Command Move/Copy started' 'Move/Copy started'
$box = Places-Box (Body-Places)
$cx = [int](($box.Left + $box.Right) / 2)
$cy = [int](($box.Top + $box.Bottom) / 2)
Ui-Mark
Ui-Step 'a window in the middle (to the right)' { Window-Drag ($cx - 8) ($cy - 8) ($cx + 8) ($cy + 8) }
Expect-Bounds '0 inside, 12 outside, 0 crossing' 'every body out by its box'
Start-Sleep -Milliseconds 300
Ui-Sync
if (Ui-NewLines | Where-Object { $_ -match '^Move/Copy Objects: \d+ bod' }) { Ui-Fail 'a body taken by an empty window' }
Write-Host 'ok   no body taken'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Move/Copy cancelled' 'cancelled'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows window bodies test'
