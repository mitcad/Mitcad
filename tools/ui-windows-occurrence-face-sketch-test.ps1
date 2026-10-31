# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/MainWindow.cpp app/MainWindow.hpp app/MainWindowVisibility.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/DocumentSession.hpp tools/cli
# check-all sources: app/MainWindowActivate.cpp
# Input-driven Windows UI test of a sketch on a face of an occurrence
# placed by a turn and a move (mitcad#505) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted); the Linux test
# is tools/ui-occurrence-face-sketch-test.sh. A design of mitcad-cli:
# Box:1 (O1) turned 90 degrees about X and moved by (100, 0, 50), with a
# 30 x 20 x 10 block whose top face (z = 10 in Box) faces -Y in the
# design, at y = -10, from x 100 to 130 and z 50 to 70, its middle at
# (115, -10, 60). Seen from the front, then orbited by a drag on the
# orientation cube right before each sketch (mitcad#1009):
#   1. Box active: Create Sketch on that face; the sketch is Box's, its
#      frame in the design on the face's plane with the normal (0, -1, 0),
#      the view looking along +Y; a line drawn from the face's middle (where
#      the view shows the block's middle) has both ends on the face.
#   2. The root active: the same in a sketch of the root linked to the face;
#      the pick asks whether to activate Box:1 (mitcad#960, "Activate
#      component asked: Box:1 (active Root)"), answered Keep in Root
#      (Alt+K).
#   3. The root still active, the question answered Activate Box:1 (Enter,
#      the default): Box is active, the sketch on the same pick is Box's.
#   4. The root active, Box:1 and the root's sketch on the face hidden and
#      the root's cross on the origin's XZ plane around the origin shown,
#      the view shows only the cross: seen from the front, a fit of it has no depth, which put an
#      orthographic camera's eye on its centre. Orbited, Create Sketch on
#      the origin's XZ plane looks against its normal, along (0, -1, 0).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-occurrence-face-sketch-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-occurrence-face-sketch'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'view.front' = 'F8' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# Box:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1
# (F2.b0), placed by the turn and the move. The root's sketches may lie on
# Box's faces (mitcad#220). The root's Sketch2 (F3): a cross of two 120 mm
# lines on the origin's XZ plane through the origin, for part 4 (the origin
# planes' pick places, a third of their 100 mm size from the origin, lie
# between its arms), hidden until then: the other parts' fits stay as close
# as without it.
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$lines = @('[',
  '  {"cmd": "create_component", "name": "Box", "transform": [[1, 0, 0, 100], [0, 0, -1, 0], [0, 1, 0, 50]]},',
  '  {"cmd": "sketch.create"},',
  '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},',
  ('  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
    '"}], "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},'),
  '  {"cmd": "activate_component", "component": "Root"},',
  '  {"cmd": "set_policy", "component": "Box", "policy": {"attachments_from_others": true}},',
  '  {"cmd": "set_policy", "component": "Root", "policy": {"attach_to_others": true}},',
  '  {"cmd": "sketch.create", "plane": "xz"},',
  '  {"cmd": "sketch.add_line", "sketch": "F3", "start": [-60, 0], "end": [60, 0]},',
  '  {"cmd": "sketch.add_line", "sketch": "F3", "start": [0, -60], "end": [0, 60]},',
  '  {"cmd": "set_feature_visible", "uid": "F3", "visible": false}',
  ']')
$script = Join-Path $work 'turned.json'
$design = Join-Path $work 'turned.mitcad'
Set-Content -Encoding ascii $script $lines
$output = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

$invariant = [Globalization.CultureInfo]::InvariantCulture

# The three numbers of "(x, y, z)".
function Xyz([string]$Text) {
  return , @($Text -split ',\s*' | ForEach-Object { [double]::Parse($_.Trim(), $invariant) })
}

# The last line logged since the mark that contains the text.
function New-Line([string]$Text) {
  $line = Ui-LogLines | Select-Object -Skip ($script:UiMark + 1) | Where-Object { $_.Contains($Text) } |
    Select-Object -Last 1
  if (-not $line) { Ui-Fail "'$Text' not in the log since the mark" }
  return "$line"
}

# Within: each coordinate within the tolerance of the expected one.
function Within([double[]]$Got, [double[]]$Want, [double]$Tolerance, [string]$Description) {
  for ($i = 0; $i -lt 3; $i++) {
    if ([Math]::Abs($Got[$i] - $Want[$i]) -gt $Tolerance) {
      Ui-Fail "${Description}: ($($Got -join ', ')), expected ($($Want -join ', ')) within $Tolerance"
    }
  }
  Write-Host "ok   ${Description}: ($($Got -join ', '))"
}

# On-Face: a point of the design on the block's top face (y = -10 within
# 1e-6 mm, inside its extent; -Plane: on the face's plane).
function On-Face([double[]]$Got, [string]$Description, [switch]$Plane) {
  $on = [Math]::Abs($Got[1] + 10) -le 1e-6
  if (-not $Plane) {
    $on = $on -and $Got[0] -ge 100 -and $Got[0] -le 130 -and $Got[2] -ge 50 -and $Got[2] -le 70
  }
  if (-not $on) { Ui-Fail "${Description}: ($($Got -join ', ')) is not on the face (y = -10, x 100 to 130, z 50 to 70)" }
  Write-Host "ok   ${Description}: ($($Got -join ', ')) on the face"
}

# Orbit where: a drag on the orientation cube's front face orbits the view
# away from the front right before a sketch is entered (mitcad#1009:
# entering a sketch kept the oblique direction of a view orbited just
# before, its up turned to the sketch's y only).
function Orbit([string]$Where) {
  $from = Ui-LoggedAt 'Orientation cube front'
  Ui-Mark
  Ui-Step 'orbit (drag the cube)'        { Ui-Drag $from[0] $from[1] ($from[0] - 120) ($from[1] + 80) }
  Ui-ExpectNew 'Camera direction' "${Where}: the drag orbits" 8
  $last = New-Line 'Camera direction'
  if ($last.Contains('direction 0 1 0 ')) { Ui-Fail "${Where}: the drag did not turn the view: $last" }
  Write-Host "ok   ${Where}: orbited to $($last.Substring($last.IndexOf('direction')))"
  Ui-Sync
}

# Front-Fit where: the front view, fitted.
function Front-Fit([string]$Where) {
  Ui-Mark
  Ui-Step 'front view (F8)'              { Ui-Key 'F8' }
  Ui-ExpectNew 'Camera direction 0 1 0 ' "${Where}: looking from the front" 8
  Ui-Step 'fit (F6)'                     { Ui-Key 'F6' }
  Ui-Sync
}

# Sketch-OnFace where [keep|activate]: Create Sketch on the block's top
# face, seen from the front and orbited from there; the frame and the view
# (along the face's inward normal) checked; a line from the face's middle
# 7 and 4 mm along the sketch's axes, its ends checked. With keep or
# activate, the pick asks whether to activate Box:1 (mitcad#960) and is
# answered so; without, it asks nothing.
function Sketch-OnFace([string]$Where, [string]$Answer = '') {
  Front-Fit $Where
  Orbit $Where
  Ui-Mark
  Ui-Step 'create sketch (F7)'           { Ui-Key 'F7' }
  Ui-ExpectNew 'Pick face F2.b0/F2:end(' "${Where}: the top face can be picked"
  Ui-Step 'pick the top face'            { Ui-ClickPick 'face F2.b0/F2:end(' }
  if ($Answer) {
    Ui-ExpectNew 'Activate component asked: Box:1 (active Root)' "${Where}: asked whether to activate Box:1"
    Ui-FocusDialog '^Activate Component$'
    if ($Answer -eq 'keep') {
      Ui-Step 'keep in Root (Alt+K)'     { Ui-Key 'alt+k' }
      Ui-FocusMain
      Ui-ExpectNew 'Activate component answer: keep in Root' "${Where}: kept in the root"
    } else {
      Ui-Step 'activate Box:1 (Enter)'   { Ui-Key 'Return' }
      Ui-FocusMain
      Ui-ExpectNew 'Activate component answer: activate Box:1' "${Where}: Box:1 to be activated"
      Ui-ExpectNew 'Activated Box' "${Where}: Box is active"
    }
  }
  Ui-ExpectNew 'Sketch started on {"body":"F2.b0","face":"F2:end(' "${Where}: a sketch on the face"
  if (-not $Answer) {
    if (Ui-LogLines | Select-Object -Skip ($script:UiMark + 1) | Where-Object { $_.Contains('Activate component asked') }) {
      Ui-Fail "${Where}: asked whether to activate a component"
    }
    Write-Host "ok   ${Where}: nothing asked"
  }
  Ui-ExpectNew '} in O1, origin (' "${Where}: picked where Box:1 shows it"
  Ui-ExpectNew 'Sketch view along' "${Where}: the view's direction logged"
  $frame = New-Line 'Sketch frame in the design'
  if ($frame -notmatch ' origin \(([^)]*)\), normal \(([^)]*)\)') { Ui-Fail "${Where}: no frame in '$frame'" }
  $origin = Xyz $Matches[1]
  $normal = Xyz $Matches[2]
  On-Face $origin "${Where}: the frame's origin in the design" -Plane
  Within $normal @(0, -1, 0) 1e-6 "${Where}: the frame's normal along the face's"
  if ((New-Line 'Sketch view along') -notmatch 'along \(([^)]*)\)') { Ui-Fail "${Where}: no view direction" }
  Within (Xyz $Matches[1]) @(0, 1, 0) 1e-6 "${Where}: Look At normal to the face"
  Ui-Sync
  Ui-Mark
  Ui-Step 'line tool (L)'                { Ui-Key 'l' }
  Ui-ExpectNew 'Sketch tool Line' "${Where}: the line tool"
  # The block's middle, seen along the face's normal, is the face's middle:
  # the sketch point the pointer shows there.
  $at = Ui-LoggedAt 'Body F2.b0 in O1'
  # MK_SHIFT keeps the posted move in session 0 (tools/ui-windows-build-test.ps1).
  Ui-Step "pointer over the block's middle" { [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0x04) }
  Ui-ExpectNew 'Sketch pointer x ' "${Where}: the pointer's sketch point"
  if ((New-Line 'Sketch pointer x ') -notmatch 'pointer x ([-0-9.]+), y ([-0-9.]+) mm') {
    Ui-Fail "${Where}: no sketch point of the pointer"
  }
  $sx = [double]::Parse($Matches[1], $invariant)
  $sy = [double]::Parse($Matches[2], $invariant)
  Ui-Step "start at the face's middle"   { Ui-SketchClick $sx $sy }
  Ui-Step 'end beside it'                { Ui-SketchClick ($sx + 7) ($sy + 4) }
  Ui-ExpectNew 'Line c1 in the design from' "${Where}: a line drawn"
  if ((New-Line 'Line c1 in the design from') -notmatch 'from \(([^)]*)\) to \(([^)]*)\)$') {
    Ui-Fail "${Where}: no line ends"
  }
  $from = Xyz $Matches[1]
  $to = Xyz $Matches[2]
  On-Face $from "${Where}: the line's start"
  On-Face $to "${Where}: the line's end"
  Within $from @(115, -10, 60) 1.5 "${Where}: the start where the face's middle was clicked"
  Ui-Step 'end the chain and the tool'   { Ui-Key 'Escape', 'Escape' }
  Ui-Mark
  Ui-Step 'finish sketch (Ctrl+Enter)'   { Ui-Key 'ctrl+Return' }
  Ui-ExpectNew 'Sketch finished' "${Where}: sketch finished"
  if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
}

# The question whether to activate a picked face's component (mitcad#960),
# which the other tests turn off.
$env:MITCAD_ASK_ACTIVATE_COMPONENT = '1'
# The orientation cube's places, for the drags that orbit.
$env:MITCAD_LOG_ORIENTATION_CUBE = '1'
Ui-StartApp 'occurrence-face-sketch' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0 in O1' "Box's block shown"

Write-Host '--- 1. Box active: a sketch of Box on its own face'
Ui-Mark
Ui-Step 'activate Box'                   { Ui-ClickLogged 'Browser radio Root/Box:1' }
Ui-ExpectNew 'Activated Box' 'Box is active'
Sketch-OnFace 'Box active'
Ui-ExpectLog 'Sketch Sketch3 is in Box' "the sketch is Box's"
Ui-ExpectLog 'Sketch frame in the design (at O1)' 'shown where Box:1 places it'

Write-Host '--- 2. The root active: a sketch of the root linked to the face'
Ui-Mark
Ui-Step 'activate the root'              { Ui-ClickLogged 'Browser radio Root' }
Ui-ExpectNew 'Activated Root' 'the root is active'
Sketch-OnFace 'the root active' 'keep'
Ui-ExpectLog 'Sketch Sketch4 is in Root' "the sketch is the root's"

Write-Host '--- 3. The root active, Box:1 activated from the question (mitcad#960)'
$framesAtO1 = @(Ui-LogLines | Where-Object { $_.Contains('Sketch frame in the design (at O1)') }).Count
Sketch-OnFace 'Box:1 activated' 'activate'
Ui-ExpectLog 'Sketch Sketch5 is in Box' "the sketch is Box's"
if (@(Ui-LogLines | Where-Object { $_.Contains('Sketch frame in the design (at O1)') }).Count -le $framesAtO1) {
  Ui-Fail 'Box:1 activated: the sketch is not shown where Box:1 places it'
}
Write-Host 'ok   Box:1 activated: shown where Box:1 places it'

Write-Host '--- 4. A sketch on an origin plane after a fit of a flat sketch and an orbit (mitcad#1009)'
Ui-Mark
Ui-Step 'activate the root'              { Ui-ClickLogged 'Browser radio Root' }
Ui-ExpectNew 'Activated Root' 'the root is active'
Ui-Mark
Ui-Step 'hide Box:1'                     { Ui-ClickLogged 'Browser eye Root/Box:1' }
Ui-ExpectNew 'Bodies shown: none' 'Box:1 hidden'
Ui-Mark
Ui-ExpectLog 'Browser eye Root/Sketches/Sketch2 at' "the root's cross in the browser"
Ui-Step 'show Sketch2'                   { Ui-ClickLogged 'Browser eye Root/Sketches/Sketch2' }
Ui-ExpectLog 'Browser eye Root/Sketches/Sketch4 at' "the root's sketch on the face in the browser"
Ui-Step 'hide Sketch4'                   { Ui-ClickLogged 'Browser eye Root/Sketches/Sketch4' }
$clock = [Diagnostics.Stopwatch]::StartNew()
do {
  $shown = Ui-NewLines | Where-Object { $_.StartsWith('Sketches shown: ') } | Select-Object -Last 1
  if ("$shown" -eq 'Sketches shown: F3') { break }
  Start-Sleep -Milliseconds 200
} while ($clock.ElapsedMilliseconds -lt 6000)
if ("$shown" -ne 'Sketches shown: F3') { Ui-Fail "origin plane: not only the cross shown: $shown" }
Write-Host 'ok   origin plane: only the cross on XZ shown'
Front-Fit 'origin plane'
Orbit 'origin plane'
Ui-CreateSketch 'F7' 'xz'
Ui-ExpectNew 'Sketch view along' "origin plane: the view's direction logged"
if ((New-Line 'Sketch frame in the design') -notmatch ' normal \(([^)]*)\)') { Ui-Fail 'origin plane: no frame' }
Within (Xyz $Matches[1]) @(0, 1, 0) 1e-6 "origin plane: the XZ plane's normal"
if ((New-Line 'Sketch view along') -notmatch 'along \(([^)]*)\)') { Ui-Fail 'origin plane: no view direction' }
Within (Xyz $Matches[1]) @(0, -1, 0) 1e-6 "origin plane: Look At against the plane's normal"
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'origin plane: sketch finished'
if (Ui-Crashed) { Ui-Fail 'the application crashed' }

Ui-Finish 'UI Windows occurrence face sketch test'
