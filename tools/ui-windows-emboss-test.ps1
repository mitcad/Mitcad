# SPDX-License-Identifier: MIT
# check-all sources: app/commands/EmbossCommands.cpp core/model/src/features/emboss.rs
# check-all sources: geometry/src/emboss_projection.cpp tools/cli
# Input-driven Windows UI test of Emboss by projection (mitcad#1091) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), on a
# design mitcad-cli makes: a ball 40 across about the origin (Sphere1, F1,
# Body1), a plane 30 above XY (Plane1, F2) and on it the text "Mitcad" 6
# high from (-9, 3) (Sketch1, F3), off the sphere's pole and seam. Emboss
# takes the newest letter, another picked beside it, then the ball's face
# (which takes the projection by itself), Depth 1.5: previewed, OK, the
# letters projected down onto the ball and raised, its volume grown, and
# Emboss1 in the timeline. Then the wrap offsets (mitcad#1259) on a rod:
# Along Axis, Around and Rotation typed, each previewed where it puts the
# rectangle (the logged middle of the preview's tool), the Along Axis arrow
# dragged, and kept by OK with the volume. The Linux counterparts are the
# Emboss by projection and wrap offsets steps of tools/ui-create-test.sh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-emboss-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-emboss'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.emboss' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'emboss-ball.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": "xy", "center": [0, 0], "diameter": 40,
    "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "sketch.create", "plane": "F2"},
  {"cmd": "sketch.add_text", "sketch": "F3", "text": "Mitcad", "at": [-9, 3], "height": 6}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'emboss' @('--open', $design)
Ui-ExpectLog 'emboss-ball.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the ball shown'

Write-Host "--- Emboss of two letters on the ball, projected"
Ui-Mark
Ui-Step 'emboss (F7)'                    { Ui-Key 'F7' }
Ui-ExpectNew 'Command Emboss started' 'the panel opened' 30
Ui-ExpectNew 'Emboss Profiles: 1 profile [profile r{t1.g' 'a letter of the text taken' 30
Ui-ExpectNew 'Pick profile F3/r{t1.g' 'a letter can be picked' 30
# The newest pick place of another letter (glyph): the taken letter is
# offered too, and a click on it removes it (mitcad#1105 gives a profile
# whose middle does not pick it a place of its own).
$embossLog = "$(Ui-LogText)"
if ($embossLog -notmatch '(?s).*Emboss Profiles: 1 profile \[profile r\{t1\.(g\d+)\.') {
  Ui-Fail 'the taken letter not in the log'
}
$takenGlyph = $Matches[1]
$placesAt = [Math]::Max(0, $embossLog.LastIndexOf('Pick places:', [StringComparison]::Ordinal))
$otherLetter = [regex]::Matches($embossLog.Substring($placesAt), 'Pick (profile F3/r\{t1\.g\d+\.c\d+\})') |
  ForEach-Object { $_.Groups[1].Value } | Where-Object { -not $_.Contains("r{t1.$takenGlyph.") } |
  Select-Object -Last 1
if (-not $otherLetter) { Ui-Fail "no pick place logged for a letter other than '$takenGlyph'" }
Write-Host "ok   another letter can be picked ($otherLetter, not $takenGlyph)"
Ui-Step 'pick another letter too'        { Ui-ClickPick $otherLetter }
Ui-ExpectNew 'Emboss Profiles: 2 profiles' 'two letters' 30
Ui-Step 'the face input'                 { Ui-ClickLogged 'Panel Emboss input face' }
Ui-Step 'pick the ball'                  { Ui-ClickPick 'face F1.b0/F1:side0' }
Ui-ExpectNew 'Emboss Face: 1 face' 'the ball picked' 30
Ui-Mark
Ui-Step 'depth 1.5'                      { Ui-TypeIn 'Panel Emboss input depth' '1.5' }
Ui-ExpectNew 'Preview Emboss: ok' 'previewed, projected' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added emboss (Raise) of 2 profile(s) on a face (sphere), projected, depth 1.5' 'the emboss added' 30
Ui-ExpectNew 'Body Body1 (F1.b0): volume 33510.322 -> ' 'raised on the ball' 30
Ui-ExpectLog 'Timeline Emboss1 at' 'Emboss1 in the timeline'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-StopApp

Write-Host "--- Emboss wrap offsets (mitcad#1259) on a rod"
# A rod of diameter 20, 30 long up Z (Sketch1 F1, Extrude1 F2), and on XZ
# (through its axis) a 6 x 6 rectangle from z = 2 to 8 (Sketch2 F3), as
# the Emboss steps of tools/ui-create-test.sh. Along Axis, Around and
# Rotation typed each preview the rectangle where they put it; its Along
# Axis arrow dragged changes Along Axis; OK keeps them and the volume (the
# rectangle moved, not changed: 36 * 21 / 20 more than the rod).
$rodScript = Join-Path $work 'rod.json'
$rodDesign = Join-Path $work 'emboss-rod.mitcad'
Set-Content -Encoding ascii $rodScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-3, -8], "width": 6, "height": 6}
]
'@
$made = & $cli run $rodScript --save $rodDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $rodDesign)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'emboss-rod' @('--open', $rodDesign)
Ui-ExpectLog 'emboss-rod.mitcad' 'the rod opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the rod shown'
Ui-Mark
Ui-Step 'emboss (F7)'                    { Ui-Key 'F7' }
Ui-ExpectNew 'Command Emboss started' 'the panel opened' 30
Ui-ExpectNew 'Emboss Profiles: 1 profile [profile r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]} of F3]' 'the rectangle taken' 30
# Emboss-ToolAt x y z tolerance what: the newest "Preview tool at" (the
# middle of the previewed tool's box in the design) within the tolerance of
# x, y, z in each coordinate.
function Emboss-ToolAt([double]$X, [double]$Y, [double]$Z, [double]$Tolerance, [string]$What) {
  $at = Ui-LastMatch 'Preview tool at (\S+)'
  $p = @("$at" -split ',' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) })
  if ($p.Count -ne 3 -or [Math]::Abs($p[0] - $X) -gt $Tolerance -or [Math]::Abs($p[1] - $Y) -gt $Tolerance -or
      [Math]::Abs($p[2] - $Z) -gt $Tolerance) {
    Ui-Fail "${What}: the preview's tool at '$at', not within $Tolerance of $X,$Y,$Z"
  }
  Write-Host "ok   ${What}: the preview's tool at $at"
}
# The face picked previews at once with the default depth (1 mm): typing 1
# changes nothing, so the mark goes before the pick.
Ui-Step 'the face input'                 { Ui-ClickLogged 'Panel Emboss input face' }
Ui-Mark
Ui-Step "pick the rod's side"            { Ui-ClickPick 'face F2.b0/F2:side(c1)' }
Ui-ExpectNew 'Emboss Face: 1 face' 'the rod picked' 30
# The handles come with the face (the overlay logs its places when they
# change, not again for a click in a field): Along Axis's arrow up the rod,
# counted towards +Z.
Ui-ExpectNew 'Manipulator Emboss along_axis at ' 'Along Axis has an arrow' 30
Ui-ExpectNew 'Arrow direction Emboss along_axis: 0.000,0.000,1.000' 'the arrow points up the rod' 30
Ui-ExpectNew 'Manipulator Emboss around_angle at ' 'Around has a ring' 30
Ui-ExpectNew 'Manipulator Emboss rotation_angle at ' 'Rotation has a ring' 30
Ui-ExpectNew 'Preview Emboss: ok' 'previewed where the sketch puts it' 30
# The 6 x 6 rectangle wrapped on the +Y side from z = 2 to 8, radius 10 to
# 11: its box's middle.
Emboss-ToolAt 0 10.277 5 0.02 'on the +Y side, z = 2 to 8'
Ui-Step 'depth 1'                        { Ui-TypeIn 'Panel Emboss input depth' '1' }
Ui-ExpectLog 'Emboss Depth: 1' 'depth 1'
Ui-Mark
Ui-Step 'Along Axis 10'                  { Ui-TypeIn 'Panel Emboss input along_axis' '10' }
Ui-ExpectNew 'Emboss Along Axis: 10' '10 mm typed' 30
Ui-ExpectNew 'Preview Emboss: ok' 'previewed 10 mm along the axis' 30
Emboss-ToolAt 0 10.277 15 0.02 '10 mm up the rod'
Ui-Mark
Ui-Step 'Around 90 deg'                  { Ui-TypeIn 'Panel Emboss input around_angle' '90 deg' }
Ui-ExpectNew 'Emboss Around: 90 deg' '90 deg typed' 30
Ui-ExpectNew 'Preview Emboss: ok' 'previewed a quarter turn around the axis' 30
Emboss-ToolAt -10.277 0 15 0.02 'a quarter turn about +Z: on the -X side'
Ui-Mark
Ui-Step 'Rotation 30 deg'                { Ui-TypeIn 'Panel Emboss input rotation_angle' '30 deg' }
Ui-ExpectNew 'Emboss Rotation: 30 deg' '30 deg typed' 30
# The rotation ring's knob moved: the handles' places logged again, the
# Along Axis arrow's where the rectangle is now.
Ui-ExpectNew 'Manipulator Emboss along_axis at ' 'the handles logged where they are now' 30
Ui-ExpectNew 'Preview Emboss: ok' 'previewed turned on the face' 30
# Turned 30 deg about its middle: 8.196 across (0.41 rad), its box's middle
# at -(11 + 10 cos 0.41) / 2 = -10.086; the box of slanted walls may be
# looser.
Emboss-ToolAt -10.086 0 15 0.3 'turned on the face about its middle'
# The Along Axis arrow dragged from its newest logged place.
$knob = Ui-LoggedAt 'Manipulator Emboss along_axis'
Ui-Mark
Ui-Step 'drag the arrow up'              { Ui-Drag $knob[0] $knob[1] $knob[0] ($knob[1] - 40) }
Ui-ExpectNew 'Manipulator Emboss along_axis dragged to' 'the arrow dragged' 30
Ui-ExpectNew 'Preview Emboss:' 'previewed where the drag put it' 30
$dragged = Ui-LastMatch 'Emboss Along Axis: .* = (-?[0-9.]+) mm'
if ($null -eq $dragged -or [double]::Parse($dragged, [Globalization.CultureInfo]::InvariantCulture) -eq 10) {
  Ui-Fail "the drag left Along Axis at '$dragged', not moved from 10"
}
Write-Host "ok   Along Axis dragged to $dragged mm"
# Then typed again, to a value it was not yet (a cached preview would log
# nothing new).
$again = if ([double]::Parse($dragged, [Globalization.CultureInfo]::InvariantCulture) -eq 11) { 12 } else { 11 }
Ui-Mark
Ui-Step "Along Axis $again"              { Ui-TypeIn 'Panel Emboss input along_axis' "$again" }
Ui-ExpectNew 'Preview Emboss: ok' "previewed $again mm along the axis" 30
Emboss-ToolAt -10.086 0 (5 + $again) 0.3 "$again mm up the rod"
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew "Added emboss (Raise) of 1 profile(s) on a cylindrical face, wrapped, depth 1, along axis $again, around 90 deg, rotation 30 deg" 'the emboss added with its offsets' 30
Ui-ExpectVolume 'Body Body1 \(F2\.b0\): volume 9424\.778 -> ([0-9.]+) mm3' 9462.577961 'raised on the rod, moved'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows emboss test'
