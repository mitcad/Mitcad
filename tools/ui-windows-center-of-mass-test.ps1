# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowCenterOfMass.cpp app/view/CenterOfMass.cpp app/view/CenterOfMassView.cpp
# check-all sources: app/browser app/OcctViewer.cpp core/model-api/src/center_of_mass.rs tools/cli
# Input-driven Windows UI test of the Center of Mass marker (mitcad#1094) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-center-of-mass-test.sh on Linux, on the same design: Body1
# (F2.b0), a 30 x 20 x 10 block of aluminum in the root, and Body2 (F3.b0)
# below it in Component1:1 (O1), steel for lack of a material.
#   1. Center of Mass (F8 here) draws the marker at the total center of
#      mass of the `properties` query (mitcad-cli info --properties, within
#      0.000001 mm); the browser's Center of Mass row under Origin, its
#      tooltip with the coordinates, the mass and the steel note.
#   2. Component1:1 moved 20 mm along X with Move/Copy from its row's menu:
#      the marker moves by 20 mm times Body2's share of the mass.
#   3. The row's light bulb hides and shows it; hidden, saved and opened
#      again it stays hidden; Delete in its menu removes it, undo brings it
#      back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-center-of-mass-test.ps1 [-App build\dev\mitcad.exe] [-Cli mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-center-of-mass'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'inspect.center_of_mass' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelJson = Join-Path $work 'model.json'
$design = Join-Path $work 'design.mitcad'
Set-Content -Encoding ascii $modelJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"}
]
'@
$output = & $Cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# The `properties` query of the design as saved: the total's center of
# mass and each body's mass.
$info = & $Cli info $design --json --properties 2>$null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'mitcad-cli info --properties failed' }
$properties = ($info -join "`n" | ConvertFrom-Json).properties
$total = @($properties.total.center_of_mass | ForEach-Object { [double]$_ })
$m1 = [double](@($properties.bodies | Where-Object { $_.uid -eq 'F2.b0' })[0].mass)
$m2 = [double](@($properties.bodies | Where-Object { $_.uid -eq 'F3.b0' })[0].mass)
if ($total.Count -ne 3 -or $m1 -le 0 -or $m2 -le 0) { Ui-Fail "no properties of both bodies: $($info -join ' ')" }
Write-Host "ok   the properties query: total center of mass $($total -join ', '); masses $m1 and $m2 kg"

$invariant = [Globalization.CultureInfo]::InvariantCulture
# Expect-Center x y z "description": the newest "Center of Mass: at x, y, z
# mm" line since the mark within 0.000001 mm of (x, y, z).
function Expect-Center([double]$X, [double]$Y, [double]$Z, [string]$Description) {
  $line = @(Ui-NewLines | Where-Object { $_ -match '^Center of Mass: at ' }) | Select-Object -Last 1
  if (-not $line) { Ui-Fail "${Description}: no 'Center of Mass: at' line" }
  if ($line -notmatch '^Center of Mass: at (-?[0-9.]+), (-?[0-9.]+), (-?[0-9.]+) mm') {
    Ui-Fail "${Description}: cannot read '$line'"
  }
  $got = @([double]::Parse($Matches[1], $invariant), [double]::Parse($Matches[2], $invariant),
           [double]::Parse($Matches[3], $invariant))
  $want = @($X, $Y, $Z)
  for ($i = 0; $i -lt 3; $i++) {
    if ([Math]::Abs($got[$i] - $want[$i]) -gt 1e-6) {
      Ui-Fail "${Description}: the marker is at $($got -join ', '), expected $($want -join ', ')"
    }
  }
  Write-Host "ok   ${Description}: $($got -join ', ')"
}

# Menu-Entries: the entries of the context menu logged last.
function Menu-Entries { return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ') }
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step "choose $Entry" {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}

Ui-StartApp 'center-of-mass' @('--open', $design)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0, F3.b0 in O1' 'both bodies shown'
Ui-ExpectLog 'Center of Mass: none' 'no marker yet'

Write-Host '--- 1. Center of Mass'
Ui-Mark
Ui-Step 'Center of Mass (F8)'              { Ui-Key 'F8' }
Ui-ExpectNew 'Center of Mass added' 'the marker added'
Ui-ExpectNew 'Center of Mass: at ' 'its place worked out'
Expect-Center $total[0] $total[1] $total[2] 'the marker at the properties query''s total'
Ui-ExpectNew 'Center of Mass drawn: ' 'drawn in the view'
Ui-ExpectNew ', 2 bodies, 1 without a material (shown)' 'two bodies, Body2 steel by default'
Ui-ExpectNew '; 1 body without a material counted as steel' 'the tooltip names the steel body'
$tip = "$(Ui-LastLineWith 'Center of Mass: at ' -Start)" -replace '^.*; tooltip: ', ''
if ($tip -notmatch '^\(-?[0-9.]+, -?[0-9.]+, -?[0-9.]+\) mm, [0-9.]+ kg; 1 body') {
  Ui-Fail "the tooltip is not the coordinates, the mass and the steel note: '$tip'"
}
Write-Host "ok   the tooltip: $tip"
Ui-ExpectLog 'Browser Root/Center of Mass at' 'the Center of Mass row in the browser'
Ui-ExpectLog 'Browser eye Root/Center of Mass at' 'with its light bulb'
$origin = [int](Ui-LastMatch '^Browser Root/Origin at \d+,(\d+)$')
$row = [int](Ui-LastMatch '^Browser Root/Center of Mass at \d+,(\d+)$')
if ($row -le $origin) { Ui-Fail "the Center of Mass row ($row) is not under Origin ($origin)" }
Write-Host 'ok   the row is under Origin'

Write-Host '--- 2. Moving a component moves the marker'
Ui-Mark
Ui-Step 'click Component1:1'               { Ui-ClickLogged 'Browser Root/Component1:1' }
Ui-ExpectNew 'Selected: 1 component' 'Component1:1 selected'
Ui-Mark
Ui-Step 'right-click Component1:1'         { Ui-ClickLogged 'Browser Root/Component1:1' 3 }
Ui-ExpectNew 'Context menu: ' 'the row''s menu'
Menu-Choose 'Move/Copy'
Ui-ExpectNew 'Panel Move/Copy choices: move_type=components' 'moving the component'
Ui-ExpectNew 'Panel Move/Copy input tx at' 'the panel is laid out'
Ui-Step '20 mm along X'                    { Ui-TypeIn 'Panel Move/Copy input tx' '20' }
Ui-ExpectNew 'Preview Move/Copy: ok' 'previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                       { Ui-Key 'Return' }
Ui-ExpectNew 'Moved 1 components (' 'Component1:1 moved' 30
Ui-ExpectNew 'Center of Mass: at ' 'the marker''s place worked out again'
Expect-Center ($total[0] + 20 * $m2 / ($m1 + $m2)) $total[1] $total[2] 'the marker moved by Body2''s share of 20 mm'
Ui-Step 'Escape (clear the selection)'     { Ui-Key 'Escape' }

Write-Host '--- 3. The light bulb, saved, Delete'
Ui-Mark
Ui-Step 'hide the marker (light bulb)'     { Ui-ClickLogged 'Browser eye Root/Center of Mass' }
Ui-ExpectNew 'Visibility Root/Center of Mass: hidden' 'hidden by its light bulb'
Ui-ExpectNew 'Center of Mass drawn: none' 'not drawn'
Ui-ExpectNew ' without a material (hidden)' 'the marker kept, hidden'
Ui-Mark
Ui-Step 'show it (light bulb)'             { Ui-ClickLogged 'Browser eye Root/Center of Mass' }
Ui-ExpectNew 'Visibility Root/Center of Mass: shown' 'shown again'
Ui-ExpectNew 'Center of Mass drawn: ' 'drawn again'
Ui-Mark
Ui-Step 'hide it again (light bulb)'       { Ui-ClickLogged 'Browser eye Root/Center of Mass' }
Ui-ExpectNew 'Visibility Root/Center of Mass: hidden' 'hidden'
Ui-Step 'save (Ctrl+S)'                    { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$saved = Get-Content -Raw $design | ConvertFrom-Json
if (-not $saved.display -or -not $saved.display.center_of_mass) { Ui-Fail 'the design file has no Center of Mass marker' }
if ($saved.display.center_of_mass.visible -ne $false) { Ui-Fail 'the design file keeps the marker shown' }
Write-Host 'ok   the design file keeps the marker, hidden'

Ui-StartApp 'center-of-mass-again' @('--open', $design)
Ui-ExpectLog ' without a material (hidden)' 'opened again: the marker there, hidden'
Ui-ExpectLog 'Browser Root/Center of Mass at' 'its row in the browser'
if (Ui-LogLines | Where-Object { $_ -match '^Center of Mass drawn: -?[0-9]' }) { Ui-Fail 'the hidden marker is drawn' }
Write-Host 'ok   not drawn'
Ui-Mark
Ui-Step 'show it (light bulb)'             { Ui-ClickLogged 'Browser eye Root/Center of Mass' }
Ui-ExpectNew 'Center of Mass drawn: ' 'drawn'
Ui-Mark
Ui-Step 'right-click the row'              { Ui-ClickLogged 'Browser Root/Center of Mass' 3 }
Ui-ExpectNew 'Context menu: Hide | Delete' 'its menu: Hide and Delete'
Menu-Choose 'Delete'
Ui-ExpectNew 'Deleted the Center of Mass marker' 'deleted'
Ui-ExpectNew 'Center of Mass drawn: none' 'no longer drawn'
Ui-ExpectNew 'Center of Mass: none' 'gone from the design'
Ui-ExpectNew 'Browser Root/Center of Mass out of sight' 'its row gone'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Delete Center of Mass' 'undo'
Ui-ExpectNew ' without a material (shown)' 'the marker back, shown'
Ui-ExpectNew 'Center of Mass drawn: ' 'and drawn'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows Center of Mass test'
