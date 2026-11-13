# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJointHighlight.cpp app/framework/JointHighlight.cpp app/framework/JointHighlight.hpp
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/browser core/model-api/src/joints.rs
# check-all sources: core/model/src/build core/model-build core/vcs/src/library tools/cli
# Input-driven Windows UI test of the joint highlight (mitcad#523) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-joint-highlight-test.sh on Linux. A design mitcad-cli makes:
# Plate:1 (a 40 x 40 x 10 block, grounded), Pin:1 (a 10 x 10 x 20 block)
# and Shaft:1 (a cylinder of diameter 6); Joint1 puts the pin's bottom face
# on the plate's top face (revolute, flipped), Joint2 the shaft's bottom
# circular edge on the plate's top face with an override that moves the
# frame to (5, 5, 10):
#   1. Joint1's row in the browser: the log names side a's face of the pin
#      and side b's face of the plate, both frames where the faces meet, the
#      two occurrences; the card names it.
#   2. Joint2's row replaces it: side a the plate's face, its frame only,
#      side b the shaft's circular edge; nothing of Joint1 is left.
#   3. Joint1's cell in the timeline highlights it as its row does; a click
#      on nothing in the view clears it.
#   4. The pointer over Joint2's row previews it; off the joints' rows (on
#      the Joints folder's), nothing.
#   5. A BUILD joint (where git is installed: a library of one synthetic
#      timber member made here, fetched from a local git repository): a
#      post on the ground and a member on its side; its joint's row shows
#      both sides' mount frames with their parts.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-joint-highlight-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-joint-highlight'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}
# Script name text: the commands run by mitcad-cli and saved as <name>.mitcad.
function Save-Design([string]$Name, [string]$Script) {
  $json = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $json $Script
  $design = Join-Path $work "$Name.mitcad"
  Run-Cli $Name @('run', $json, '--save', $design)
  return $design
}
# The last "Joint highlight <side>: " line since the mark starts with
# $Start and ends with $End.
function Expect-Side([string]$Side, [string]$Start, [string]$End, [string]$Description) {
  Ui-ExpectNew "Joint highlight ${Side}: $Start" $Description
  $line = Ui-LogLines | Select-Object -Skip $script:UiMark |
    Where-Object { $_.StartsWith("Joint highlight ${Side}: ") } | Select-Object -Last 1
  if (-not "$line".EndsWith($End)) { Ui-Fail "${Description}: '$line' does not end with '$End'" }
}
# Fails when a line since the mark contains $Text.
# A design taller than the browser opens folded below the root
# (mitcad#426), its Joints folder closed: opened, so that the joints' rows
# are laid out (their places logged again; the components stay folded).
function Open-Joints {
  if (Ui-LogLines | Where-Object { $_.StartsWith('Browser opened folded') }) {
    Ui-Mark
    Ui-Step 'open the Joints folder'     { Ui-ClickLogged 'Browser arrow Root/Joints' }
    Ui-ExpectNew 'Browser Root/Joints/Joint1 at ' "the joints' rows in sight"
  }
}
function Expect-None([string]$Text, [string]$Description) {
  if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) }) {
    Ui-Fail "${Description}: '$Text' is in the log"
  }
  Write-Host "ok   $Description"
}

# Plate (C1, Plate:1 O1): Sketch1 F1, Extrude1 F2 (Body1); Pin (C2, Pin:1
# O2): Sketch2 F3, Extrude2 F4 (Body2); Shaft (C3, Shaft:1 O3): Cylinder1
# F5 (Body3); Joint1 F6, Joint2 F7.
$joints = Save-Design 'joints' (@'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Shaft", "transform": {"translation": [-50, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 6, "height": 30, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"},
  {"cmd": "add_joint", "kind": "revolute", "flip": true,
    "a": {"occurrence": "Pin:1", "geometry": {"body": "F4.b0", "face": "F4:start(RECT)"}},
    "b": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"}}},
  {"cmd": "add_joint", "kind": "rigid",
    "a": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"},
          "frame_override": {"origin": [5, 5, 10]}},
    "b": {"occurrence": "Shaft:1", "geometry": {"body": "F5.b0", "edge": "E{F5:bottom|F5:side0}"}}},
  {"expect": {"query": {"query": "joints"}, "result": {"joints": [
    {"name": "Joint1", "state": "placed", "a": {"on": "face", "path": ["O2"]}, "b": {"on": "face"}},
    {"name": "Joint2", "state": "placed", "b": {"on": "edge", "component": "C3"}}]}}}
]
'@).Replace('RECT', $rect)

Write-Host '--- Joint1 from its row in the browser: a face on a face'
Ui-StartApp 'joint-highlight' @('--open', $joints)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser joints: Joint1 placed, Joint2 placed' 'the browser has both joints'
Open-Joints
Ui-Mark
Ui-Step "click Joint1's row"             { Ui-ClickLogged 'Browser Root/Joints/Joint1' }
Ui-ExpectNew 'Selected: 1 feature [feature F6]' 'Joint1 is selected'
Ui-ExpectNew "Joint highlight a: face F4:start($rect) of F4.b0 in Pin:1 (O2) [Joint1]" "side a: the pin's bottom face, in the pin"
Ui-ExpectNew "Joint highlight b: face F2:end($rect) of F2.b0 in Plate:1 (O1) [Joint1]" "side b: the plate's top face, in the plate"
Ui-ExpectNew 'Joint highlight frame a at 20.0,20.0,10.0 [Joint1]' "side a's frame where the faces meet"
Ui-ExpectNew 'Joint highlight frame b at 20.0,20.0,10.0 [Joint1]' "side b's frame there too"
Ui-ExpectNew 'Joint highlight occurrences: Pin:1 (O2), Plate:1 (O1) [Joint1]' 'the two occurrences it joins'
Ui-ExpectNew 'Joint card: Joint1: revolute, a = Pin:1, face of Body2' 'the card names the joint and side a in words'
Ui-ExpectNew ', b = Plate:1, face of Body1' 'and side b'

Write-Host '--- Joint2 replaces it: a frame override and a circular edge'
Ui-Mark
Ui-Step "click Joint2's row"             { Ui-ClickLogged 'Browser Root/Joints/Joint2' }
Ui-ExpectNew 'Selected: 1 feature [feature F7]' 'Joint2 is selected'
Ui-ExpectNew "Joint highlight a: face F2:end($rect) of F2.b0 in Plate:1 (O1), frame only [Joint2]" "side a: the plate's face moved by an override shows its frame only"
Ui-ExpectNew 'Joint highlight b: edge E{F5:bottom|F5:side0} of F5.b0 in Shaft:1 (O3) [Joint2]' "side b: the shaft's circular edge"
Ui-ExpectNew 'Joint highlight frame a at 5.0,5.0,10.0 [Joint2]' "the override's frame"
Ui-ExpectNew 'Joint highlight occurrences: Plate:1 (O1), Shaft:1 (O3) [Joint2]' 'the plate and the shaft'
Ui-ExpectNew '(its frame)' 'the card says side a is its frame'
Expect-None '[Joint1]' 'nothing of Joint1 is highlighted'

Write-Host '--- Joint1 from the timeline, then cleared'
Ui-Mark
Ui-Step 'click Joint1 in the timeline'   { Ui-ClickLogged 'Timeline Joint1' }
Ui-ExpectNew "Joint highlight a: face F4:start($rect) of F4.b0 in Pin:1 (O2) [Joint1]" "the timeline's cell highlights it as its row does"
Ui-Mark
Ui-Step 'click on nothing in the view'   { Ui-ViewClick 3 50 }
Ui-ExpectNew 'Joint highlight: none' 'deselecting clears the highlight'

Write-Host "--- The pointer over Joint2's row previews it"
Ui-Mark
# A move with nothing in wParam is taken by Qt for the "enter" of a window
# shown under the cursor and dropped while no window is under the pointer,
# which in session 0 (no cursor) is the case here: MK_SHIFT in wParam keeps
# the move (Qt reads the modifiers from the key state, so it is a plain
# move), as in tools/ui-windows-build-test.ps1.
Ui-Step "point at Joint2's row" {
  $at = Ui-LoggedAt 'Browser Root/Joints/Joint2'
  [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0x04)
}
Ui-ExpectNew 'Joint hover preview: Joint2' 'the row under the pointer is previewed'
Expect-None 'Selected: ' 'the hover selected nothing'
# Off the row: in session 0 the browser gets its Leave right after each
# posted move (no cursor stays over it), so the preview clears at once:
# the hover's Joint2 line is followed by "none" (BrowserPanel::hoverJoint;
# the Linux test moves to the view, where the Leave comes with the move).
$lines = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
$shown = [Array]::IndexOf($lines, 'Joint hover preview: Joint2')
$cleared = $shown -ge 0 -and @($lines | Select-Object -Skip ($shown + 1) | Where-Object { $_ -eq 'Joint hover preview: none' }).Count -gt 0
if (-not $cleared) {
  for ($i = 0; $i -lt 25 -and -not $cleared; $i++) {
    Start-Sleep -Milliseconds 200
    $lines = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
    $shown = [Array]::IndexOf($lines, 'Joint hover preview: Joint2')
    $cleared = $shown -ge 0 -and @($lines | Select-Object -Skip ($shown + 1) | Where-Object { $_ -eq 'Joint hover preview: none' }).Count -gt 0
  }
}
if (-not $cleared) { Ui-Fail 'the preview does not go with the pointer: no "Joint hover preview: none" after Joint2''s' }
Write-Host 'ok   the preview goes with the pointer'

Write-Host '--- A BUILD joint: a member on a post''s side'
$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; the library cannot be fetched, the BUILD joint is not tested'
  Ui-Finish 'UI Windows joint highlight test'
  exit 0
}
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
$library = Join-Path $work 'test-timber'
Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-timber', '--name', 'Test timber', '--license', 'MIT')
function Frame([string]$Name, [string]$Origin, [string]$Z, [string]$X) {
  return ('  {"cmd": "add_feature", "name": "mount:' + $Name + '", "def": {"type": "joint_origin", "geometry": "origin", ' +
          '"frame_override": {"origin": ' + $Origin + ', "z_axis": ' + $Z + ', "x_axis": ' + $X + '}}},')
}
# A timber member 300 wide (along X) and 200 thick (along Y), centred on
# the Z axis, extruded by L from z = 0, with its end frames and a face
# frame on each side (docs/libraries.md, "Timber parts").
$member = @('[',
  '  {"cmd": "add_parameter", "name": "L", "expression": "600 mm"},',
  '  {"cmd": "sketch.create", "plane": "xy"},',
  '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-150, -100], "width": 300, "height": 200},',
  ('  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect + '"}], ' +
   '"extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},'),
  (Frame 'end_a' '[0, 0, 0]' '[0, 0, -1]' '[1, 0, 0]'),
  ('  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin", ' +
   '"geometry": {"body": "F2.b0", "face": "F2:end(' + $rect + ')"}}},'),
  (Frame 'face_xp' '[150, 0, 0]' '[1, 0, 0]' '[0, 0, 1]'),
  (Frame 'face_xn' '[-150, 0, 0]' '[-1, 0, 0]' '[0, 0, 1]'),
  (Frame 'face_yp' '[0, 100, 0]' '[0, 1, 0]' '[0, 0, 1]'),
  (Frame 'face_yn' '[0, -100, 0]' '[0, -1, 0]' '[0, 0, 1]'),
  '  {"expect": {"bodies": 1}}',
  ']') -join "`n"
$memberJson = Join-Path $work 'member.json'
Set-Content -Encoding ascii $memberJson $member
New-Item -ItemType Directory -Force (Join-Path $library 'dimensioned-timber') | Out-Null
Run-Cli 'member' @('run', $memberJson, '--save', (Join-Path $library 'dimensioned-timber\timber-200-test.mitcad'))
Set-Content -Encoding ascii (Join-Path $library 'mitcad-library.json') @'
{
  "format": "mitcad-library", "version": 1, "id": "test-timber", "name": "Test timber",
  "description": "A synthetic BUILD part of the joint highlight UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "dimensioned-timber", "name": "Dimensioned timber"}],
  "components": [
    {"id": "timber-200-test", "path": "dimensioned-timber/timber-200-test.mitcad", "name": "Timber 200x300 test",
     "category": "dimensioned-timber", "designation": "Timber 200x300 test L={L}",
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
       "availability": "standard",
       "section": {"width": "300 mm", "height": "200 mm"}, "nominal": {"width": "300 mm", "height": "200 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "600 mm",
                  "stock": ["3000 mm", "3600 mm", "4200 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}]}}
  ]
}
'@
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)
# The post (O1, its derived part F1) on the ground at the origin, 1200
# long; the member (O2, F2) on the post's +X side 300 up, 600 long; its
# joint Joint1 (F3) in the root.
# The path as library fetch had it, escaped for JSON.
$libraryUrl = $library.Replace('\', '\\')
$timber = Save-Design 'timber' (@'
[
  {"cmd": "build_place", "part": {"library": "test-timber", "url": "URL", "rev": "HEAD", "component": "timber-200-test"},
   "at": {"point": [0, 0, 0]}, "length": 1200},
  {"cmd": "build_place", "part": {"source": "S1"},
   "at": {"occurrence": "O1", "geometry": {"mount": "F1:mount(face_xp)"}, "offset": 300}, "length": 600},
  {"expect": {"query": {"query": "joints"}, "result": {"joints": [
    {"name": "Joint1", "a": {"on": "mount", "path": ["O2"]}, "b": {"on": "mount", "path": ["O1"]}}]}}}
]
'@).Replace('URL', $libraryUrl)

Ui-StartApp 'joint-highlight-build' @('--open', $timber)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser joints: Joint1 ' "the browser has the member's joint"
Open-Joints
Ui-Mark
Ui-Step "click the member's joint"       { Ui-ClickLogged 'Browser Root/Joints/Joint1' }
Expect-Side 'b' 'mount frame face_xp of F1 in ' ' (O1), frame and part [Joint1]' "side b: the post's side frame, with the post"
Expect-Side 'a' 'mount frame ' ' (O2), frame and part [Joint1]' "side a: the member's own frame, with the member"
Ui-ExpectNew 'Joint highlight frame a at ' "side a's frame is drawn"
Ui-ExpectNew 'Joint highlight frame b at ' "side b's frame is drawn"
$occurrences = Ui-LogLines | Select-Object -Skip $script:UiMark |
  Where-Object { $_.StartsWith('Joint highlight occurrences: ') } | Select-Object -Last 1
if ("$occurrences" -notmatch ' \(O2\), .* \(O1\) \[Joint1\]$') { Ui-Fail "the joined parts: $occurrences" }
Write-Host "ok   the two parts: $occurrences"
Ui-ExpectNew 'Joint card: Joint1: ' 'the card names the BUILD joint'

Ui-Finish 'UI Windows joint highlight test'
