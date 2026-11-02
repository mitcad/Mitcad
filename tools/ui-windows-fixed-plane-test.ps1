# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PlaceInputs.cpp app/commands/PrimitiveCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/PulleyCommands.cpp app/commands/GearCommands.cpp app/commands/BearingCommands.cpp
# check-all sources: app/commands/SweepCommands.cpp app/commands/HoleCommands.cpp app/framework/Selection.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/CommandSession.cpp app/MainWindow.cpp tools/cli
# Input-driven Windows UI test of features placed on a fixed plane
# (mitcad#1137) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted); the Linux counterpart is tools/ui-fixed-plane-test.sh.
# mitcad-cli makes a 60 x 40 x 20 block (Sketch1 F1, Extrude1 F2) and on
# the fixed plane at Z 36 (`{"origin": [0, 0, 36], "normal": [0, 0, 1]}`,
# as importers and MCP agents write it): Cylinder1 (F3, centre (100, 0)),
# Box1 (F4, corner (100, 30)), Sphere1 (F5), Torus1 (F6), Coil1 (F7),
# Pulley1 (F8), Gear1 (F9), Bearing1 (F10); Hole1 (F11) by `at` on a fixed
# plane at Z 20, Box2 (F12) by `at` on the fixed plane at Z 36:
#   1. Every feature type Place On places opens for editing, its Place On
#      row listing "Plane at Z 36 mm (fixed)", drawn in the view.
#   2. Cylinder1: the diameter 12, OK: the same fixed plane and centre.
#   3. Box1: the height 6, OK: the same fixed plane and corner. Edited
#      again, the block's top face picked in Place On: saved with `at` on
#      that face, the fixed plane gone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fixed-plane-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fixed-plane'),
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
$modelJson = Join-Path $work 'model.json'
Set-Content -Encoding ascii $modelJson (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": Z36, "center": [100, 0],
    "diameter": 10, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": Z36, "corner": [100, 30],
    "length": 10, "width": 8, "height": 4, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": Z36, "center": [130, 0],
    "diameter": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "torus", "plane": Z36, "center": [160, 0],
    "diameter": 20, "section_diameter": 4, "position": "on_center", "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "coil", "plane": Z36, "center": [200, 0], "diameter": 20,
    "helix": {"type": "revolutions_and_pitch", "revolutions": 2, "pitch": 8}, "angle": 0,
    "clockwise": false, "section": "circular", "section_position": "on_center", "section_size": 2,
    "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "pulley", "kind": "toothed", "profile": "gt2_2mm", "plane": Z36,
    "center": [0, 80], "teeth": 20, "belt_width": 6, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "gear", "kind": "spur", "plane": Z36, "center": [0, -80],
    "module": 2, "teeth": 20, "face_width": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "printable_bearing", "size": "608", "plane": Z36, "center": [60, 80],
    "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole", "placement": {"type": "at", "at": {
      "face": {"origin": [0, 0, 20], "normal": [0, 0, 1]}, "edge1": "x", "offset1": 20, "edge2": "y", "offset2": 30,
      "near": [30, 20, 20]}}, "diameter": 6, "extent": {"type": "through_all"}, "participants": ["F2.b0"]}},
  {"cmd": "add_feature", "def": {"type": "box", "length": 5, "width": 5, "height": 5, "operation": "new_body",
    "at": {"face": {"origin": [0, 0, 36], "normal": [0, 0, 1], "x_axis": [1, 0, 0]},
           "edge1": "x", "offset1": 50, "edge2": "y", "offset2": 40, "near": [40, -50, 36]}}}
]
'@).Replace('RECT', $rect).Replace('Z36', '{"origin": [0, 0, 36], "normal": [0, 0, 1]}')
$design = Join-Path $work 'fixed.mitcad'
$output = & $cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

Ui-StartApp 'fixed-plane' @('--open', $design)
Start-Sleep -Milliseconds 800
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the design shown'

Write-Host '--- 1. Every placed feature type opens on its fixed plane'
# The feature's own row in the timeline, its uid, its command and its
# plane's text. The step's block finds $fixedRow in this function's scope
# (not $Name: Ui-Step's own parameter would hide it).
function Edit-Fixed([string]$Feature, [string]$Uid, [string]$Command, [string]$Plane) {
  $fixedRow = "Timeline $Feature"
  Ui-Mark
  Ui-Step "edit $Feature (double-click)" { Ui-DoubleClickLogged $fixedRow }
  Ui-ExpectNew "Editing $Uid with $Command" "$Feature opens for editing" 30
  Ui-ExpectNew "$Command Place On listed: $Plane" "$Feature on the fixed plane, listed as $Plane"
  Ui-ExpectNew "Place ${Command}: face $Plane" "$Feature placed on it"
  Ui-Mark
  Ui-Step 'cancel (Esc)'                 { Ui-Key 'Escape' }
  Ui-ExpectNew "Command $Command cancelled" "the edit of $Feature cancelled"
}
Edit-Fixed 'Cylinder1' 'F3' 'Cylinder' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Box1' 'F4' 'Box' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Sphere1' 'F5' 'Sphere' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Torus1' 'F6' 'Torus' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Coil1' 'F7' 'Coil' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Pulley1' 'F8' 'Pulley' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Gear1' 'F9' 'Gear' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Bearing1' 'F10' 'Bearing' 'Plane at Z 36 mm (fixed)'
Edit-Fixed 'Hole1' 'F11' 'Hole' 'Plane at Z 20 mm (fixed)'
Edit-Fixed 'Box2' 'F12' 'Box' 'Plane at Z 36 mm (fixed)'
if ((Ui-LogLines) -match 'cannot be edited') { Ui-Fail 'a feature on a fixed plane was refused' }
Write-Host 'ok   none refused'

Write-Host '--- 2. Cylinder1: a new diameter keeps the fixed plane'
Ui-Mark
Ui-Step 'edit Cylinder1 (double-click)'  { Ui-DoubleClickLogged 'Timeline Cylinder1' }
Ui-ExpectNew 'Editing F3 with Cylinder' 'Cylinder1 opens for editing' 30
Ui-ExpectNew 'Cylinder Place On listed: Plane at Z 36 mm (fixed)' 'on the fixed plane'
# The plane drawn as a construction plane is, about its origin (0, 0, 36).
Ui-ExpectNew 'View highlight places: ' "the highlights' places logged"
$drawn = $false
for ($i = 0; $i -lt 25 -and -not $drawn; $i++) {
  $line = Ui-LastLineWith 'View highlight places: '
  $drawn = @("$line" -split ';' | Where-Object { $_ -match 'plane fixed/.* at 0\.0,0\.0,36\.0' }).Count -gt 0
  if (-not $drawn) { Start-Sleep -Milliseconds 200 }
}
if (-not $drawn) { Ui-Fail "the fixed plane not drawn at Z 36: $line" }
Write-Host 'ok   the fixed plane drawn where it lies'
Ui-Mark
Ui-Step 'diameter 12'                    { Ui-TypeIn 'Panel Cylinder input diameter' '12' }
Ui-ExpectNew 'Preview Cylinder: ok' '12 wide previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F3' 'Cylinder1 edited' 30
Ui-ExpectVolume 'Body \S+ \(F3\.b0\): volume [0-9.]+ -> ([0-9.]+) mm3' ([Math]::PI * 36 * 20) '12 wide, 20 high'

Write-Host '--- 3. Box1: a new height keeps it, a picked face replaces it'
Ui-Mark
Ui-Step 'edit Box1 (double-click)'       { Ui-DoubleClickLogged 'Timeline Box1' }
Ui-ExpectNew 'Editing F4 with Box' 'Box1 opens for editing' 30
Ui-ExpectNew 'Box Place On listed: Plane at Z 36 mm (fixed)' 'on the fixed plane'
Ui-Mark
Ui-Step 'height 6'                       { Ui-TypeIn 'Panel Box input height' '6' }
Ui-ExpectNew 'Preview Box: ok' '6 high previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F4' 'Box1 edited' 30
Ui-FocusMain
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'

# Numbers compared as numbers: Windows PowerShell 5.1 reads the file's
# `100.0` as a decimal whose text is "100.0", not "100".
function Same($Values, [double[]]$Expected) {
  $v = @($Values)
  if ($v.Count -ne $Expected.Count) { return $false }
  for ($i = 0; $i -lt $v.Count; $i++) {
    if ([math]::Abs([double]$v[$i] - $Expected[$i]) -gt 1e-6) { return $false }
  }
  return $true
}
# The fixed plane `{"origin": [0, 0, 36], "normal": [0, 0, 1]}`, as written.
function Is-Z36($Plane) {
  return $Plane -and (Same $Plane.origin 0, 0, 36) -and (Same $Plane.normal 0, 0, 1)
}
$saved = Get-Content -Raw $design | ConvertFrom-Json
# A size typed in a panel is saved as its parameter (`"diameter": "d4"`):
# its millimetres are the parameter's saved `value`.
function Size-Of($Value) {
  if ($Value -is [string]) {
    $p = @($saved.parameters | Where-Object { $_.name -eq $Value })[0]
    if ($p) { return [double]$p.value }
    return $null
  }
  return $Value
}
$cylinder = @($saved.features | Where-Object { $_.type -eq 'cylinder' })[0]
if (-not (Is-Z36 $cylinder.plane) -or -not (Same $cylinder.center 100, 0) -or (Size-Of $cylinder.diameter) -ne 12 -or
    $cylinder.PSObject.Properties['at']) {
  Ui-Fail ("Cylinder1 not on its fixed plane: $($cylinder | ConvertTo-Json -Compress -Depth 8); " +
           "plane $(Is-Z36 $cylinder.plane), centre '$(@($cylinder.center) -join ',')', diameter $(Size-Of $cylinder.diameter); " +
           "parameters: $($saved.parameters | ConvertTo-Json -Compress -Depth 4)")
}
Write-Host 'ok   Cylinder1 saved with its fixed plane and centre, 12 wide'
$box = @($saved.features | Where-Object { $_.type -eq 'box' })[0]
if (-not (Is-Z36 $box.plane) -or -not (Same $box.corner 100, 30) -or (Size-Of $box.height) -ne 6 -or
    $box.PSObject.Properties['at']) {
  Ui-Fail "Box1 not on its fixed plane: $($box | ConvertTo-Json -Compress -Depth 8)"
}
Write-Host 'ok   Box1 saved with its fixed plane and corner, 6 high'

Ui-Mark
Ui-Step 'edit Box1 (double-click)'       { Ui-DoubleClickLogged 'Timeline Box1' }
Ui-ExpectNew 'Editing F4 with Box' 'Box1 opens again' 30
Ui-Mark
Ui-Step 'Place On'                       { Ui-ClickLogged 'Panel Box input at' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew "Pick face F2.b0/F2:end($rect) at" "the block's top face can be picked"
Ui-Mark
Ui-Step "click the block's top face"     { Ui-ClickPick "face F2.b0/F2:end($rect) at" }
Ui-ExpectNew 'Place Box: face face F2:end(' 'the top face in place of the fixed plane'
Ui-ExpectNew 'Preview Box: ok' 'previewed on the block' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F4' 'Box1 placed on the block' 30
Ui-FocusMain
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$saved = Get-Content -Raw $design | ConvertFrom-Json
$box = @($saved.features | Where-Object { $_.type -eq 'box' })[0]
if ($box.at.face.body -ne 'F2.b0' -or $box.PSObject.Properties['plane']) {
  Ui-Fail "Box1 not on the face: $($box | ConvertTo-Json -Compress -Depth 8)"
}
Write-Host 'ok   Box1 saved by a reference to the top face, the fixed plane gone'
if ((Ui-LogLines) -match 'Recompute failed') { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows fixed plane test'
