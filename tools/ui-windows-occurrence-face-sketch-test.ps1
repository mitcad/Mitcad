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
# (115, -10, 60). Seen from the front:
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
# Box's faces (mitcad#220).
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$lines = @('[',
  '  {"cmd": "create_component", "name": "Box", "transform": [[1, 0, 0, 100], [0, 0, -1, 0], [0, 1, 0, 50]]},',
  '  {"cmd": "sketch.create"},',
  '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},',
  ('  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
    '"}], "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},'),
  '  {"cmd": "activate_component", "component": "Root"},',
  '  {"cmd": "set_policy", "component": "Box", "policy": {"attachments_from_others": true}},',
  '  {"cmd": "set_policy", "component": "Root", "policy": {"attach_to_others": true}}',
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

# Sketch-OnFace where [keep|activate]: Create Sketch on the block's top
# face, seen from the front; the frame and the view checked; a line from
# the face's middle 7 and 4 mm along the sketch's axes, its ends checked.
# With keep or activate, the pick asks whether to activate Box:1
# (mitcad#960) and is answered so; without, it asks nothing.
function Sketch-OnFace([string]$Where, [string]$Answer = '') {
  Ui-Step 'front view (F8)'              { Ui-Key 'F8' }
  Ui-Step 'fit (F6)'                     { Ui-Key 'F6' }
  Ui-Sync
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
Ui-StartApp 'occurrence-face-sketch' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0 in O1' "Box's block shown"

Write-Host '--- 1. Box active: a sketch of Box on its own face'
Ui-Mark
Ui-Step 'activate Box'                   { Ui-ClickLogged 'Browser radio Root/Box:1' }
Ui-ExpectNew 'Activated Box' 'Box is active'
Sketch-OnFace 'Box active'
Ui-ExpectLog 'Sketch Sketch2 is in Box' "the sketch is Box's"
Ui-ExpectLog 'Sketch frame in the design (at O1)' 'shown where Box:1 places it'

Write-Host '--- 2. The root active: a sketch of the root linked to the face'
Ui-Mark
Ui-Step 'activate the root'              { Ui-ClickLogged 'Browser radio Root' }
Ui-ExpectNew 'Activated Root' 'the root is active'
Sketch-OnFace 'the root active' 'keep'
Ui-ExpectLog 'Sketch Sketch3 is in Root' "the sketch is the root's"

Write-Host '--- 3. The root active, Box:1 activated from the question (mitcad#960)'
$framesAtO1 = @(Ui-LogLines | Where-Object { $_.Contains('Sketch frame in the design (at O1)') }).Count
Sketch-OnFace 'Box:1 activated' 'activate'
Ui-ExpectLog 'Sketch Sketch4 is in Box' "the sketch is Box's"
if (@(Ui-LogLines | Where-Object { $_.Contains('Sketch frame in the design (at O1)') }).Count -le $framesAtO1) {
  Ui-Fail 'Box:1 activated: the sketch is not shown where Box:1 places it'
}
Write-Host 'ok   Box:1 activated: shown where Box:1 places it'

Ui-Finish 'UI Windows occurrence face sketch test'
