# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework app/browser app/view core/model-api/src core/model/src/document tools/cli
# check-all sources: core/model-drawing/src third_party/fonts/iso-lettering
# Input-driven Windows UI test of annotations (mitcad#702) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-annotations-test.sh on Linux, on the design of
# tools/ui-windows-measurement-objects-test.ps1. The Annotations window is
# driven by its keys (the input goes to the main window's places only).
#   1. Kept: AxisDistance1 (50) and Diameter1 (Ø10).
#   2. A double-click on AxisDistance1's text: its editor, a ± tolerance.
#   3. Its text dragged: moved.
#   4. The Annotations window (F8 here): Diameter1's editor without a Plane
#      row, Diameter1 given an H7 fit; both
#      green, two decimals, in the group Shop; Diameter1 in Inspect; Shop
#      hidden and shown again.
#   5. Export Snapshot (F9 here) of the group Inspect.
#   6. Datum Label… on Body1's top face and Frame… on Body2's round face in the
#      Measure panel (perpendicularity, Ø0.05 Ⓜ, to A); AxisDistance1's row
#      highlighting its faces; Add Frame… (Alt+F) under it, parallelism to
#      A. (Picking again from the editor is in the Linux test only: its
#      buttons are in a dialog.)
#   7. Saved and opened again.
#   8. Hole Callout in the Measure panel on a plate with four counterbored
#      through holes (mitcad#902): HoleCallout1 written from the hole's
#      feature, in the bundled lettering.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-annotations-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-annotations'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'view.annotations' = 'F8'; 'view.export_snapshot' = 'F9' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelJson = Join-Path $work 'model.json'
$design = Join-Path $work 'axes.mitcad'
Set-Content -Encoding ascii $modelJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 40], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [20, -30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F5", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
'@
$output = & $cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

function Wait-Enabled([string]$Place) {
  $last = $null
  # Looked for every 50 ms (200 ms before, mitcad#870), up to 6 s.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $last = Ui-LastLineWith "$Place at "
    if ($last -and -not "$last".EndsWith('(disabled)')) { return }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Ui-Fail "'$Place' is not enabled: $last"
}

# Annotate "letter" "entry": once the Measure panel offers the leaders for
# its pick, its Annotate menu opened and the entry chosen by its letter
# (mitcad#1010).
function Annotate([string]$Letter, [string]$Entry) {
  Ui-ExpectNew 'Measure Annotate entries: Note' 'the pick''s leaders offered'
  Ui-Mark
  Ui-Step 'Annotate'                       { Ui-ClickLogged 'Measure Annotate' }
  Ui-ExpectNew 'Context menu: Note' 'Annotate''s menu'
  Ui-Step "$Entry ($Letter)"               { Ui-Key $Letter }
}

# Keys to the Annotations window.
function Window-Keys([string[]]$Keys) {
  Ui-FocusDialog '^Annotations$'
  foreach ($key in $Keys) { Ui-Key $key }
}

$o = [char]0x00D8 # the diameter's prefix
$pm = [char]0x00B1 # plus-minus

Ui-StartApp 'annotations' @('--open', $design)
Ui-ExpectLog 'Annotations: none' 'no annotations yet'
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cylinders shown'

Write-Host '--- 1. Kept'
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
Ui-ExpectNew 'Command Measure started' 'measure started'
Ui-Step 'pick Body1''s round face'         { Ui-ClickPick 'face F2.b0/F2:side(c1)' }
Ui-Step 'pick Body2''s round face'         { Ui-ClickPick 'face F4.b0/F4:side(c1)' }
Ui-ExpectNew 'axis distance 50' 'the axes 50 apart'
Wait-Enabled 'Measure Keep as Dimension'
Ui-Step 'Keep as Dimension'                { Ui-ClickLogged 'Measure Keep as Dimension' }
Ui-ExpectNew 'Annotation kept: AxisDistance1' 'kept as the axes'' distance'
Ui-Step 'close (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Measure' 'measure closed'
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
# Started before its pick, as the Linux test waits for it.
Ui-ExpectNew 'Command Measure started' 'measure started again'
Ui-Step 'pick Body1''s top rim'            { Ui-ClickPick 'edge F2.b0/E{F2:end(r{c1})|' }
Ui-ExpectNew 'Inspect Measure: length 31.416, radius 5, diameter 10' 'the rim measured'
Wait-Enabled 'Measure Keep as Dimension'
Ui-Step 'Keep as Dimension'                { Ui-ClickLogged 'Measure Keep as Dimension' }
Ui-ExpectNew 'Annotation kept: Diameter1' 'kept as the rim''s diameter'
Ui-ExpectNew "Annotations drawn: AxisDistance1 = 50 (auto) | Diameter1 = ${o}10 (auto)" 'both drawn, in Auto'
Ui-Step 'close (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Measure' 'measure closed'

Write-Host '--- 2. A tolerance from the editor, opened in the view'
Ui-WaitIdle
Ui-Mark
Ui-Step 'double-click AxisDistance1''s text' { Ui-DoubleClickLogged 'Annotation label AxisDistance1' }
Ui-ExpectNew 'Edit Annotation dialog opened: AxisDistance1 axis_distance' 'its editor opened'
Ui-Step 'Tolerance: symmetric (Alt+T, Down), OK' {
  Ui-FocusDialog '^Edit Annotation$'
  Ui-Key 'alt+t'
  Ui-Key 'Down'
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew "Annotations drawn: AxisDistance1 = 50 ${pm}0.1 (auto)" 'drawn with its tolerance'

Write-Host '--- 3. Its text dragged'
$from = Ui-LoggedAt 'Annotation label AxisDistance1'
Ui-Mark
Ui-Step 'drag the text 80 px right'       { Ui-Drag $from[0] $from[1] ($from[0] + 80) $from[1] }
Ui-ExpectNew 'Annotation AxisDistance1 dragged: text by' 'the text dragged'
Ui-ExpectNew 'Annotation AxisDistance1 moved: ' 'moved, an undo step'
Ui-ExpectNew 'Annotation label AxisDistance1 at' 'its label placed again'
$to = Ui-LoggedAt 'Annotation label AxisDistance1'
$dx = $to[0] - $from[0]
$dy = $to[1] - $from[1]
if ($dx * $dx + $dy * $dy -le 900) { Ui-Fail "the label stayed at $($to -join ','), was $($from -join ',')" }

Write-Host '--- 4. The Annotations window'
Ui-Mark
Ui-Step 'Annotations (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'Annotations window opened: 2 annotation(s)' 'the window opened'
Ui-Step 'Diameter1''s row (type Diam)'    { Window-Keys @(); Ui-Type 'Diam' }
Ui-ExpectNew 'Annotations selected: Diameter1' 'Diameter1 selected'
Ui-Step 'Edit (Alt+E)'                    { Ui-Key 'alt+e' }
Ui-ExpectNew 'Edit Annotation dialog opened: Diameter1 diameter' 'its editor opened'
# A diameter lies in its circle's plane: no Plane row (mitcad#713).
Ui-ExpectNew 'Edit Annotation pick a at' 'its pick''s button placed'
Ui-Sync
if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Edit Annotation pick plane') }) {
  Ui-Fail 'a plane offered for a diameter'
}
Write-Host 'ok   no Plane row for a diameter'
Ui-Step 'Tolerance: fit (Alt+T, Down x4), OK' {
  Ui-FocusDialog '^Edit Annotation$'
  Ui-Key 'alt+t'
  foreach ($i in 1..4) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
Ui-ExpectNew "Diameter1 = ${o}10 H7 (auto)" 'drawn with its fit code'
Ui-Mark
Ui-Step 'select both (Ctrl+A)'            { Window-Keys @('ctrl+a') }
Ui-ExpectNew 'Annotations selected: AxisDistance1, Diameter1' 'both selected'
Ui-Step 'Colour (Alt+C): Dark Grey' {
  Ui-Key 'alt+c'
  Ui-FocusDialog '^Annotation Colour$'
  # From Auto, the first row: Black, Dark Grey (mitcad#902: no bright
  # presets).
  foreach ($i in 1..2) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
Ui-ExpectNew "Annotations drawn: AxisDistance1 = 50 ${pm}0.1 (#495057) | Diameter1 = ${o}10 H7 (#495057)" 'both dark grey'
Ui-Step 'Decimals (Alt+D): 2' {
  Window-Keys @('alt+d')
  Ui-FocusDialog '^Annotation Decimals$'
  foreach ($i in 1..3) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
Ui-ExpectNew "Annotations drawn: AxisDistance1 = 50.00 ${pm}0.1 (#495057) | Diameter1 = ${o}10.00 H7 (#495057)" 'two decimals'
Ui-Step 'Group (Alt+R): Shop' {
  Window-Keys @('alt+r')
  Ui-FocusDialog '^Annotation Group$'
  Ui-Key 'ctrl+a'
  Ui-Type 'Shop'
  Ui-Key 'Return'
}
Ui-ExpectNew 'Annotations: AxisDistance1 axis_distance in Shop | Diameter1 diameter in Shop' 'both in Shop'
Ui-Mark
Ui-Step 'Diameter1 alone (type Diam)'     { Window-Keys @(); Ui-Type 'Diam' }
Ui-ExpectNew 'Annotations selected: Diameter1' 'Diameter1 selected'
Ui-Step 'Group (Alt+R): Inspect' {
  Ui-Key 'alt+r'
  Ui-FocusDialog '^Annotation Group$'
  Ui-Key 'ctrl+a'
  Ui-Type 'Inspect'
  Ui-Key 'Return'
}
Ui-ExpectNew 'Annotations: AxisDistance1 axis_distance in Shop | Diameter1 diameter in Inspect' 'Diameter1 in Inspect'
Ui-Mark
Ui-Step 'filter Shop (Alt+G, Down x3)'    { Window-Keys @('alt+g', 'Down', 'Down', 'Down') }
Ui-ExpectNew 'Annotations group filter: Shop' 'the filter on Shop'
Ui-Step 'Hide Group (Alt+U)'              { Window-Keys @('alt+u') }
Ui-ExpectNew "Annotations drawn: Diameter1 = ${o}10.00 H7 (#495057)" 'only Inspect''s drawn'
Ui-Step 'Show Group (Alt+W)'              { Window-Keys @('alt+w') }
Ui-ExpectNew "Annotations drawn: AxisDistance1 = 50.00 ${pm}0.1 (#495057) | Diameter1 = ${o}10.00 H7 (#495057)" 'both again'
Ui-Step 'close the window (Escape)'       { Window-Keys @('Escape'); Ui-FocusMain }

Write-Host '--- 5. A snapshot of the group Inspect'
$area = Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)'
if (-not $area) { Ui-Fail 'the app did not log its view area' }
$view = $area -split ' ' | ForEach-Object { [int]$_ }
$png = Join-Path $work 'inspect.png'
Ui-Mark
Ui-Step 'export snapshot (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Snapshot dialog opened: size 2x, transparent off, annotations all (2 shown)' 'the dialog opened'
Ui-Step 'type the file' {
  Ui-FocusDialog '^Export Snapshot$'
  Ui-Key 'ctrl+a'
  Ui-Type $png
}
Ui-Step 'Annotations: Inspect (Alt+A, Down Down)' { Ui-Key 'alt+a'; Ui-Key 'Down'; Ui-Key 'Down' }
Ui-ExpectNew 'Snapshot dialog: annotations = group Inspect' 'the group chosen'
Ui-Step 'OK (Enter)'                      { Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew "Snapshot saved ${png}: $(2 * $view[2]) x $(2 * $view[3]), opaque, 1 annotation(s) of group Inspect" 'the group''s annotation only' 30
if (-not (Test-Path $png)) { Ui-Fail 'no snapshot file' }

Write-Host '--- 6. A datum label and feature control frames'
$perp = [char]0x27C2  # perpendicularity
$par = [char]0x2225   # parallelism
$mmc = [char]0x24C2   # maximum material
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
Ui-ExpectNew 'Command Measure started' 'measure started'
Ui-Step 'pick Body1''s top face'           { Ui-ClickPick 'face F2.b0/F2:end(r{c1})' }
Annotate 'd' 'Datum Label'
Ui-ExpectNew 'Datum Label dialog opened: face F2:end(r{c1}) of F2.b0 A' 'the first free letter offered'
Ui-Step 'OK (Enter)'                       { Ui-FocusDialog '^Datum Label$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Annotation kept: DatumA' 'the datum label added'
Ui-Step 'close (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Measure' 'measure closed'
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
Ui-ExpectNew 'Command Measure started' 'measure started again'
Ui-Step 'pick Body2''s round face'         { Ui-ClickPick 'face F4.b0/F4:side(c1)' }
Annotate 'f' 'Frame'
Ui-ExpectNew 'Feature Control Frame dialog opened: face F4:side(c1) of F4.b0 {}' 'the frame asked for'
Ui-Step 'perpendicularity, cylindrical, maximum material, to A' {
  Ui-FocusDialog '^Feature Control Frame$'
  foreach ($i in 1..7) { Ui-Key 'Down' }
  Ui-Key 'alt+y'
  Ui-Key 'alt+m'
  Ui-Key 'Down'
  Ui-Key 'alt+d'
  Ui-Type 'A'
  Ui-Key 'alt+m'
  Ui-Key 'Return'
  Ui-FocusMain
}
Ui-ExpectNew 'Feature Control Frame dialog: {"characteristic":"perpendicularity","datums":["A"],"diameter":true,"modifier":"M","value":0.05}' 'the frame chosen'
Ui-ExpectNew 'Annotation kept: Frame1' 'the frame added'
Ui-ExpectNew "Frame1 = $perp | ${o}0.05 $mmc | A (auto)" 'drawn in its cells, in Auto'
Ui-Step 'close (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Measure' 'measure closed'
Ui-Mark
Ui-Step 'Annotations (F8)'                { Ui-Key 'F8' }
Ui-ExpectLog "Annotations row Frame1: frame, $perp | ${o}0.05 $mmc | A, ${o}0.05 $mmc, -, auto" 'the frame listed'
Ui-Step 'AxisDistance1''s row (type Axis)' { Window-Keys @(); Ui-Type 'Axis' }
Ui-ExpectNew 'Annotations selected: AxisDistance1' 'AxisDistance1 selected'
Ui-ExpectNew 'Annotation picks highlighted: AxisDistance1: face F2:side(c1) of F2.b0; face F4:side(c1) of F4.b0' 'its faces highlighted'
Ui-Step 'Add Frame (Alt+F): parallelism to A' {
  Ui-Key 'alt+f'
  Ui-FocusDialog '^Feature Control Frame$'
  foreach ($i in 1..6) { Ui-Key 'Down' }
  Ui-Key 'alt+d'
  Ui-Type 'A'
  Ui-Key 'alt+m'
  Ui-Key 'Return'
}
Ui-ExpectNew 'Frame added under AxisDistance1' 'the frame under the dimension'
Ui-ExpectNew "Frame2 = $par | 0.05 | A (#" 'drawn under the dimension'
Ui-Step 'close the window (Escape)'       { Window-Keys @('Escape'); Ui-FocusMain }

Write-Host '--- 7. Saved and opened again'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                   { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
foreach ($field in @('"group": "Shop"', '"group": "Inspect"', '"code": "H7"', '"text_offset"', '"letter": "A"',
                     '"characteristic": "perpendicularity"', '"on": "AxisDistance1"')) {
  if (-not (Select-String -Path $design -SimpleMatch -Pattern $field -Quiet)) { Ui-Fail "the design file has no $field" }
}
Ui-StartApp 'annotations-again' @('--open', $design)
Ui-ExpectLog 'Annotations: AxisDistance1 axis_distance in Shop | Diameter1 diameter in Inspect' 'read back in their groups'
Ui-ExpectLog "Annotations drawn: AxisDistance1 = 50.00 ${pm}0.1 (#495057) | Diameter1 = ${o}10.00 H7 (#495057)" 'drawn as they were'

Write-Host '--- 8. A hole callout from the Measure panel'
$holesJson = Join-Path $work 'holes.json'
$holes = Join-Path $work 'holes.mitcad'
Set-Content -Encoding ascii $holesJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F2.b0", "face": "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})",
                  "points": [[10, 10, 10], [50, 10, 10], [10, 30, 10], [50, 30, 10]]},
    "diameter": 6.6, "extent": {"type": "through_all"},
    "kind": {"type": "counterbore", "diameter": 11, "depth": 6.8}}}
]
'@
$output = & $cli run $holesJson --save $holes 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli (the plate): $($output -join ' / ')" }
$cbore = [char]0x2334 # counterbore
$depth = [char]0x21A7 # depth
Ui-StartApp 'annotations-holes' @('--open', $holes)
Ui-ExpectLog 'Annotations: none' 'the plate opened without annotations'
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
Ui-ExpectNew 'Command Measure started' 'measure started'
Ui-Step 'pick a hole''s counterbore wall' { Ui-ClickPick 'face F2.b0/F3:hole0.cbore_wall' }
Annotate 'c' 'Hole Callout'
Ui-ExpectNew 'Measure Hole Callout clicked' 'Hole Callout clicked'
Ui-ExpectNew 'Annotation kept: HoleCallout1' 'the hole callout added'
Ui-ExpectNew "Annotations drawn: HoleCallout1 = 4x ${o}6.6 THRU / $cbore ${o}11 $depth 6.8 (auto)" 'written from the hole''s feature'
Ui-ExpectLog 'Annotation lettering: ' 'the annotations'' lettering chosen' | Out-Null
# The bundled lettering when the build has it (third_party/fonts/iso-lettering).
if (Test-Path (Join-Path (Split-Path $script:UiApp) 'fonts\lettering.ttf')) {
  $lettering = Ui-LastLineWith 'Annotation lettering: '
  if (-not $lettering -or -not $lettering.Contains('lettering.ttf')) { Ui-Fail "not the bundled lettering: $lettering" }
}
Ui-Step 'close (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Measure' 'measure closed'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                   { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
if (-not (Select-String -Path $holes -SimpleMatch -Pattern '"kind": "hole_callout"' -Quiet)) {
  Ui-Fail 'the design file has no hole callout'
}

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows annotations test'
