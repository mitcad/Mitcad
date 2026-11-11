# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/commands app/MainWindowSketch3d.cpp app/MainWindow.cpp
# check-all sources: app/MainWindow.hpp app/MainWindowSelection.cpp app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: app/framework/ModelShapes.cpp app/view/ViewController.cpp tools/cli
# check-all sources: core/model-api/src/sketch3d.rs core/sketch/src/spatial.rs core/sketch/src/spatial_convert.rs
# check-all sources: core/model/src/document/sketch_conversion.rs core/model/src/features/spatial_path.rs
# check-all sources: core/model/src/features/path.rs core/model/src/features/pipe.rs
# check-all sources: tools/ui-scripts/sketch3d-202.json
# Input-driven Windows UI test of the scenario of GitHub mitcad/Mitcad#202
# (mitcad#1550, item 7) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), as tools/ui-sketch3d-202-test.sh does on Linux, on
# tools/ui-scripts/sketch3d-202.json's design: Box1 (F1.b0) in the root,
# its top face at z 20; the component Post (O1) with a box (F2.b0) whose
# top corners are at z 50.
#   1. Create Sketch (Alt+S, 2D) on Box1's top face; a 2-Point Rectangle by
#      two clicks.
#   2. The Sketch Palette's 3D Sketch box: Sketch1 (F3) becomes a 3D sketch
#      in place, its frame still on the face.
#   3. The Home view (F11, the test's key) and Fit.
#   4. Line (L) from a corner A of the rectangle by typed offsets 10, 0, 15
#      (one key per step); Esc until the tool ends.
#   5. Tangent Arc (F9, the test's key) continuing it at B, its end clicked
#      on a top corner V of Post's box: the Linked Point snap names the
#      vertex in Post, the end attached through O1; Esc until the tool
#      ends; Finish.
#   6. Pipe (F8, the test's key: the command search's popup closes at once
#      in session 0) with Chain Selection off along the line and the arc,
#      listed from A to V, smooth, an 8 mm pipe of the route's length; OK;
#      saved.
#   7. mitcad-cli on the saved design: Sketch1 is still a 3D sketch, the
#      pipe's path is its two curves, its start face at A, its end face at
#      V (the measure query).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch3d-202-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch3d-202'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.arc_tangent' = 'F9'; 'view.home' = 'F11'; 'solid.pipe' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$design = Join-Path $work 'route.mitcad'
$made = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts\sketch3d-202.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

$invariant = [Globalization.CultureInfo]::InvariantCulture
# Ui-Xyz "x, y, z": the three numbers of a logged "(x, y, z)".
function Ui-Xyz([string]$Text) {
  $parts = $Text -split ',' | ForEach-Object { [double]::Parse($_.Trim(), $invariant) }
  if ($parts.Count -ne 3) { Ui-Fail "not three coordinates: '$Text'" }
  return , $parts
}
# Ui-LastNew "regex": the first group of the last line since the mark that
# matches.
function Ui-LastNew([string]$Pattern) {
  $regex = New-Object System.Text.RegularExpressions.Regex $Pattern
  $lines = @(Ui-NewLines)
  for ($i = $lines.Count - 1; $i -ge 0; $i--) {
    $match = $regex.Match($lines[$i])
    if ($match.Success) { return $match }
  }
  return $null
}
# Ui-EscTool "Tool": Esc until the 3D sketch tool stops (its typed field,
# its chain, then the tool, one step back each), the sketch staying open.
function Ui-EscTool([string]$Tool) {
  for ($i = 1; $i -le 4; $i++) {
    Ui-Mark
    Ui-Step "Esc ($Tool, step $i)"     { Ui-Key 'Escape' }
    Ui-ExpectNew '3D sketch Esc: ' 'Esc steps back'
    Ui-SyncOr 500
    if ((Ui-NewLines) | Where-Object { $_.Contains("3D sketch tool ${Tool}: stopped") }) {
      if ((Ui-NewLines) | Where-Object { $_.Contains('Sketch finished') }) { Ui-Fail 'Esc left the sketch' }
      Write-Host "ok   the $Tool tool ends, the sketch stays open"
      return
    }
  }
  Ui-Fail "Esc did not end the $Tool tool"
}

Ui-StartApp 'sketch3d-202' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: ' 'the bodies are shown'

Write-Host '--- A 2D sketch on Box1''s top face, a rectangle on it'
Ui-Mark
Ui-Step 'create sketch (Alt+S)'          { Ui-Key 'alt+s' }
Ui-ExpectNew 'Create Sketch kind: 2D' '2D is the default'
Ui-ExpectNew 'Create Sketch: select a plane' 'Create Sketch asks for a plane or a face'
Ui-ExpectNew 'Pick face F1.b0/F1:top at' 'Box1''s top face can be picked'
Ui-SyncOr 500
Ui-Step 'pick the top face'              { Ui-ClickPick 'face F1.b0/F1:top at' }
Ui-ExpectNew 'Sketch started on {"body":"F1.b0","face":"F1:top"}' 'a 2D sketch on the face'
Ui-ExpectNew 'Sketch view ' 'the sketch view is logged'
Ui-ExpectNew 'Palette palette3dSketch at' 'the Sketch Palette''s 3D Sketch box'
Ui-SyncOr 500
Ui-Mark
Ui-Step '2-Point Rectangle (R)'          { Ui-Key 'r' }
Ui-Step 'first corner'                   { Ui-SketchClick 25 25 }
Ui-Step 'opposite corner'                { Ui-SketchClick 45 40 }
Ui-ExpectNew 'Added rectangle ' 'a rectangle on the face'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-SyncOr 500

Write-Host '--- 3D Sketch checked: Sketch1 becomes a 3D sketch in place'
Ui-Mark
Ui-Step 'check 3D Sketch'                { Ui-ClickLogged 'Palette palette3dSketch' }
Ui-ExpectNew 'Palette: 3D Sketch checked for F3' 'the box asks for the conversion'
Ui-ExpectNew 'Converted Sketch1 (F3) to a 3D sketch: dof ' 'converted in place'
Ui-ExpectNew 'View banner: 3D Sketch Sketch1' 'the 3D editor takes it, under its own name'
Ui-ExpectNew '3D sketch frame in the design: ' 'its frame is logged'
$frame = Ui-LastNew '^3D sketch frame in the design: origin \(([^)]*)\), x axis \(1\.000000, 0\.000000, 0\.000000\), normal \(0\.000000, 0\.000000, 1\.000000\)$'
if (-not $frame) { Ui-Fail "the frame is not the face's: $(Ui-LastLineWith '3D sketch frame in the design: ')" }
$origin = Ui-Xyz $frame.Groups[1].Value
if ([Math]::Abs($origin[2] - 20) -gt 1e-6) { Ui-Fail "the frame is not on the top face: $($frame.Value)" }
Write-Host 'ok   the frame stays on the face, 20 up, X and the face''s normal'
Ui-ExpectNew '3D sketch entity F3/p' 'the rectangle''s corners are drawn where a click picks them'
$corner = $null
foreach ($line in @(Ui-NewLines)) {
  if ($line -match '^3D sketch entity F3/(p[0-9]+) at ') { $corner = $Matches[1]; break }
}
if (-not $corner) { Ui-Fail 'no corner of the rectangle logged' }
Write-Host "ok   the route starts at the corner $corner"

Write-Host '--- The Home view and Fit: the face seen from above at an angle'
Ui-Mark
Ui-Step 'Home (F11)'                     { Ui-Key 'F11' }
Ui-ExpectNew 'View home' 'the Home view'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectNew "3D sketch entity F3/$corner at" 'the corner logged again where it is now'
Ui-SyncOr 500

Write-Host '--- The line AB off the face by typed offsets'
Ui-Mark
Ui-Step 'Line (L)'                       { Ui-Key 'l' }
Ui-ExpectNew '3D sketch tool Line: started' 'Line draws in the converted sketch'
Ui-ExpectNew 'Pick vertex F2.b0/V{' 'the vertices of Post''s box are logged where a click meets them'
Ui-Mark
Ui-Step "click the corner $corner"       { Ui-ClickLogged "3D sketch entity F3/$corner" }
Ui-ExpectNew '3D sketch Line: pending point 1 at (' 'the start is pending at the corner'
$pending = Ui-LastNew '^3D sketch Line: pending point 1 at \(([^)]*)\)'
if (-not $pending) { Ui-Fail 'the pending corner has no coordinates in the log' }
$a = Ui-Xyz $pending.Groups[1].Value
if ([Math]::Abs($a[2]) -gt 1e-6) { Ui-Fail "the corner is not in the face's plane: $($pending.Value)" }
Write-Host "ok   A at ($($pending.Groups[1].Value)), in the face's plane"
Ui-ExpectNew '3D sketch field dx at' 'the Offsets fields are shown'
Ui-Mark
Ui-Step 'type Delta X 10'                { Ui-ClickLogged '3D sketch field dx'; Ui-Type '10' }
Ui-ExpectNew '3D sketch field dx: 10' 'Delta X typed'
Ui-Step 'Tab to Delta Y'                 { Ui-Key 'Tab' }
Ui-ExpectNew '3D sketch field focus: dy' 'Tab reached Delta Y'
Ui-Step 'type Delta Y 0'                 { Ui-Type '0' }
Ui-ExpectNew '3D sketch field dy: 0' 'Delta Y typed'
Ui-Step 'Tab to Delta Z'                 { Ui-Key 'Tab' }
Ui-ExpectNew '3D sketch field focus: dz' 'Tab reached Delta Z'
Ui-Step 'type Delta Z 15'                { Ui-Type '15' }
Ui-ExpectNew '3D sketch field dz: 15' 'Delta Z typed'
Ui-Mark
Ui-Step 'Enter'                          { Ui-Key 'Return' }
Ui-ExpectNew '"offsets":["10' 'the typed values go as offsets'
Ui-ExpectNew '3D sketch Line: made c' 'AB made, one step'
$line = Ui-LastNew '^3D sketch Line: made (c[0-9]+) from (p[0-9]+) to (p[0-9]+)$'
if (-not $line -or $line.Groups[2].Value -ne $corner) { Ui-Fail "AB does not start at the corner ${corner}: $($line.Value)" }
$ab = $line.Groups[1].Value
$b = $line.Groups[3].Value
Write-Host "ok   AB is $ab from $corner to $b"
Ui-ExpectNew "3D sketch entity F3/$b at" 'B is drawn where a click picks it'
Ui-EscTool 'Line'

Write-Host '--- A tangent arc from B onto a top corner of Post''s box'
Ui-Mark
Ui-Step 'Tangent Arc (F9)'               { Ui-Key 'F9' }
Ui-ExpectNew '3D sketch tool Tangent Arc: started' 'Tangent Arc draws in the 3D sketch'
Ui-ExpectNew 'Pick vertex F2.b0/V{' 'Post''s vertices logged where a click meets them'
Ui-SyncOr 500
# A top corner of Post's box among the places of the last "Pick places:".
$log = "$(Ui-LogText)"
$places = $log.Substring([Math]::Max(0, $log.LastIndexOf('Pick places:', [StringComparison]::Ordinal)))
$vertex = [regex]::Match($places, '(?m)^Pick vertex F2\.b0/(V\{[^ ]*F2:top[^ ]*\}) at \d+,\d+\r?$')
if (-not $vertex.Success) { Ui-Fail 'no top corner of Post''s box to snap to' }
$vname = $vertex.Groups[1].Value
Write-Host "ok   the top corner $vname"
Ui-Mark
Ui-Step 'click B, AB''s end'             { Ui-ClickLogged "3D sketch entity F3/$b" }
Ui-ExpectNew "3D sketch Tangent Arc: continues $ab at $b" 'the arc continues AB at B'
Ui-Mark
Ui-Step 'click the corner of Post'       { Ui-ClickPick "vertex F2.b0/$vname at" }
Ui-ExpectNew "3D sketch snap: Linked Point: vertex $vname of F2.b0 in Post at (" 'the vertex of another component snaps, its badge names it and Post'
$snap = Ui-LastNew '^3D sketch snap: Linked Point: vertex .* at \(([^)]*)\)$'
if (-not $snap) { Ui-Fail 'the vertex snap has no coordinates in the log' }
$v = Ui-Xyz $snap.Groups[1].Value
Ui-ExpectNew '"cmd":"sketch3d.add_arc"' 'one sketch3d.add_arc'
Ui-ExpectNew '"mode":"tangent"' 'a tangent arc'
Ui-ExpectNew """vertex"":""$vname""" 'its end on the vertex'
Ui-ExpectNew '"occurrence":"O1"' 'through Post''s occurrence'
Ui-ExpectNew '3D sketch Tangent Arc: made c' 'the arc made, one step'
$arc = Ui-LastNew '^3D sketch Tangent Arc: made (c[0-9]+) from (p[0-9]+) to (p[0-9]+)$'
if (-not $arc -or $arc.Groups[2].Value -ne $b) { Ui-Fail "the arc does not start at B (${b}): $($arc.Value)" }
$r = $arc.Groups[1].Value
$vid = $arc.Groups[3].Value
Write-Host "ok   the arc is $r from $b to $vid at ($($snap.Groups[1].Value))"
Ui-EscTool 'Tangent Arc'
Ui-Mark
Ui-Step 'finish (Ctrl+Enter)'            { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'finished'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

# Where A and V are in the design (the frame's origin, X and the face's
# normal), V a top corner of Post's box; the route's length and the pipe's
# volume: AB (10, 0, 15), then the arc from B along AB's direction to V:
# the chord BV at an angle phi to that direction, the arc 2 phi round,
# |BV| phi / sin phi long.
$da = @(($origin[0] + $a[0]), ($origin[1] + $a[1]), ($origin[2] + $a[2]))
$dv = @(($origin[0] + $v[0]), ($origin[1] + $v[1]), ($origin[2] + $v[2]))
$isCorner = ([Math]::Abs($dv[0] - 80) -lt 1e-3 -or [Math]::Abs($dv[0] - 100) -lt 1e-3) -and
            ([Math]::Abs($dv[1] - 60) -lt 1e-3 -or [Math]::Abs($dv[1] - 80) -lt 1e-3) -and
            [Math]::Abs($dv[2] - 50) -lt 1e-3
if (-not $isCorner) { Ui-Fail "V ($($dv -join ', ')) is not a top corner of Post's box" }
Write-Host "ok   V at ($($dv -join ', ')) in the design, a top corner of Post's box"
$abLength = [Math]::Sqrt(10 * 10 + 15 * 15)
$cx = $v[0] - ($a[0] + 10)
$cy = $v[1] - $a[1]
$cz = $v[2] - ($a[2] + 15)
$chord = [Math]::Sqrt($cx * $cx + $cy * $cy + $cz * $cz)
$phi = [Math]::Acos((10 * $cx + 15 * $cz) / ($abLength * $chord))
$volume = [Math]::PI * 16 * ($abLength + $chord * $phi / [Math]::Sin($phi))

Write-Host '--- Pipe along AB and the arc'
Ui-Mark
Ui-Step 'Pipe (F8)'                      { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Pipe input chain at' 'the Pipe panel'
Ui-ExpectNew "Sketch entity F3/$ab at" 'the route''s curves are where a click picks them'
Ui-SyncOr 500
Ui-Mark
Ui-Step 'Chain Selection off'            { Ui-ClickLogged 'Panel Pipe input chain' }
Ui-ExpectNew 'Pipe: Chain Selection = off' 'the picks are taken as they are (A also joins the rectangle)'
Ui-SyncOr 500
Ui-Mark
Ui-Step 'pick AB'                        { Ui-ClickLogged "Sketch entity F3/$ab" }
Ui-ExpectNew "Pipe Path listed: $ab ($corner to $b)" 'AB from A'
Ui-SyncOr 500
Ui-Mark
Ui-Step 'pick the arc'                   { Ui-ClickLogged "Sketch entity F3/$r" }
Ui-ExpectNew "Pipe Path listed: $ab, $r ($corner to $vid)" 'the route from the corner to the vertex'
Ui-ExpectNew 'Preview Pipe: ok' 'previewed'
# The corners are logged when they change: AB alone had none, and the
# tangent arc adds none, so the last count stays 0.
$cornersLine = Ui-LastLineWith 'Pipe: the path has '
if ($cornersLine -notlike 'Pipe: the path has 0 sharp corners*') { Ui-Fail "the route is not smooth: $cornersLine" }
Write-Host 'ok   the tangent arc makes the route smooth'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added pipe' 'Pipe1 added'
Ui-ExpectVolume 'New body \S+ \(F4\.b0\): volume ([0-9.]+) mm3' $volume 'an 8 mm pipe along the whole route' 1e-3
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- The saved design: a 3D sketch, the pipe from A to V'
$f = { param($x) $x.ToString('0.000000', $invariant) }
$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @"
[
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {"name": "Sketch1", "type": "spatial_sketch", "status": "ok"}}},
  {"expect": {"query": {"query": "feature", "uid": "F4"}, "result": {"name": "Pipe1", "type": "pipe", "status": "ok",
    "def": {"path": {"spatial_sketch": "F3", "curves": ["$ab", "$r"]}}}}},
  {"expect": {"query": {"query": "measure", "a": {"body": "F4.b0", "face": "F4:start"}},
    "result": {"a": {"plane": {"origin": [$(& $f $da[0]), $(& $f $da[1]), $(& $f $da[2])]}}}, "tolerance": 1e-4}},
  {"expect": {"query": {"query": "measure", "a": {"body": "F4.b0", "face": "F4:end"}},
    "result": {"a": {"plane": {"origin": [$(& $f $dv[0]), $(& $f $dv[1]), $(& $f $dv[2])]}}}, "tolerance": 1e-4}}
]
"@
$checked = & $Cli run $check --open $design 2>&1
if ($LASTEXITCODE -ne 0) {
  Ui-Fail "the saved design: not a 3D sketch, or the pipe not from the corner to Post's vertex: $($checked -join ' / ')"
}
Write-Host "ok   Sketch1 stayed a 3D sketch; the pipe's path is $ab and $r of it, from A to Post's vertex V"

Ui-Finish 'UI Windows 3D sketch of GitHub mitcad/Mitcad#202'
