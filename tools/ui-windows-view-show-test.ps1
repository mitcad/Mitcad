# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowVisibility.cpp app/view/ViewController.cpp
# check-all sources: app/files/MainWindowLocks.cpp core/model/src/document/display.rs
# check-all sources: core/model-api/src/lib.rs tools/cli
# Input-driven Windows UI test of a body's Browser row and View > Show
# (mitcad#1287, GitHub mitcad/Mitcad#142) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-view-show-test.sh on Linux, on the same designs:
#   1. Docked: three boxes written to STEP and read back as one base
#      feature, its bodies renamed Left (F1.b0), Middle (F1.b1) and Right
#      (F1.b2). Delete Body in Middle's row menu removes Middle only (a
#      Remove Body, the base feature kept); one undo brings it back. Left
#      and Right selected, the row menu's Delete Bodies removes both in one
#      undo step.
#   2. Floating: components Frame (O1) and Motor (O2), each with a
#      construction plane (FramePlane, MotorPlane) and a sketch (F2, F4),
#      the Origin folder's light bulb on. View > Show > Construction off
#      (F8 here) hides both planes, greys their rows and keeps the sketches
#      and the origin; the design saved keeps it as display state and no
#      feature's light bulb; on again shows both planes. Sketches (F9) and
#      Origins (F7) the same way, the Origin folder's light bulb staying on.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-show-test.ps1 [-App build\dev\mitcad.exe] [-Cli mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-show'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'view.show_origins' = 'F7'; 'view.show_construction' = 'F8'; 'view.show_sketches' = 'F9' } `
  -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$step = (Join-Path $work 'boxes.step') -replace '\\', '/'
$boxesJson = Join-Path $work 'boxes.json'
Set-Content -Encoding ascii $boxesJson @"
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-60, 0], "length": 20,
    "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, 0], "length": 20,
    "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0], "length": 20,
    "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "export", "path": "$step"}
]
"@
$importedJson = Join-Path $work 'imported.json'
Set-Content -Encoding ascii $importedJson @"
[
  {"cmd": "import_file", "path": "$step", "structure": "base"},
  {"expect": {"bodies": 3}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Left"},
  {"cmd": "rename_body", "uid": "F1.b1", "name": "Middle"},
  {"cmd": "rename_body", "uid": "F1.b2", "name": "Right"}
]
"@
$componentsJson = Join-Path $work 'components.json'
Set-Content -Encoding ascii $componentsJson @'
[
  {"cmd": "create_component", "name": "Frame"},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 10}}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Motor", "transform": [[1, 0, 0, 60], [0, 1, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "rename_feature", "uid": "F1", "name": "FramePlane"},
  {"cmd": "rename_feature", "uid": "F3", "name": "MotorPlane"}
]
'@
$imported = Join-Path $work 'imported.mitcad'
$components = Join-Path $work 'components.mitcad'
foreach ($run in @(@($boxesJson, (Join-Path $work 'boxes.mitcad')), @($importedJson, $imported),
                   @($componentsJson, $components))) {
  $output = & $Cli run $run[0] --save $run[1] 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $(Split-Path -Leaf $run[0]): $($output -join ' / ')" }
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
# Last-New "prefix": the last line since the mark that begins with it.
function Last-New([string]$Prefix) {
  return @(Ui-NewLines | Where-Object { $_.StartsWith($Prefix) }) | Select-Object -Last 1
}
# Expect-Planes "description": the last "Construction shown:" line has both
# components' planes.
function Expect-Planes([string]$Description) {
  $line = "$(Ui-LastLineWith 'Construction shown: ' -Start)"
  if (-not ($line.Contains('FramePlane in O') -and $line.Contains('MotorPlane in O'))) { Ui-Fail "${Description}: $line" }
  Write-Host "ok   ${Description}: $line"
}

Write-Host '=== 1. Delete on a body''s row (Docked)'
Ui-StartApp 'view-show-delete' @('--open', $imported)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0, F1.b1, F1.b2' 'the three imported bodies shown'
Ui-ExpectLog 'View Show menu: Origins off | Construction on | Sketches on' 'View > Show''s entries, Origins unchecked while the Origin folder''s light bulb is off (mitcad#1642)'
Ui-ExpectLog 'Browser Root/Bodies/Middle at' 'the browser lists Middle'

Ui-Mark
Ui-Step 'right-click Middle''s row'        { Ui-ClickLogged 'Browser Root/Bodies/Middle' 3 }
Ui-ExpectNew 'Context menu: ' 'the row''s menu'
if ((Menu-Entries) -notcontains 'Delete Body') { Ui-Fail "the row's menu has no Delete Body: $((Menu-Entries) -join ' | ')" }
Write-Host 'ok   the row''s menu says Delete Body'
Menu-Choose 'Delete Body'
Ui-ExpectNew 'Deleted body Middle' 'Middle deleted'
Ui-ExpectNew 'Bodies shown: F1.b0, F1.b2' 'Left and Right stay: the base feature is kept'
if (Last-New 'Deleted Base') { Ui-Fail 'the base feature was deleted' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Delete Middle' 'one undo step, named as Delete Body names it'
Ui-ExpectNew 'Bodies shown: F1.b0, F1.b1, F1.b2' 'Middle back'

Ui-Mark
Ui-Step 'click Left'                       { Ui-ClickLogged 'Browser Root/Bodies/Left' }
Ui-Step 'Ctrl+click Right' {
  $at = Ui-LoggedAt 'Browser Root/Bodies/Right'
  Ui-ClickWith 'ctrl' $at[0] $at[1]
}
Ui-ExpectNew 'Browser selected: Root/Bodies/Left, Root/Bodies/Right' 'Left and Right selected'
Ui-Step 'right-click Left'                 { Ui-ClickLogged 'Browser Root/Bodies/Left' 3 }
Ui-ExpectNew 'Context menu: ' 'the row''s menu'
Menu-Choose 'Delete Bodies'
Ui-ExpectNew 'Deleted body ' 'the bodies deleted'
$deleted = "$(Last-New 'Deleted body ')"
if (-not ($deleted.Contains('Left') -and $deleted.Contains('Right')) -or $deleted.Contains('Middle')) {
  Ui-Fail "not Left and Right: $deleted"
}
Write-Host "ok   $deleted"
Ui-ExpectNew 'Bodies shown: F1.b1' 'Middle stays'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Delete Bodies' 'one undo step for both'
Ui-ExpectNew 'Bodies shown: F1.b0, F1.b1, F1.b2' 'both back'

Write-Host '=== 2. View > Show of the whole design (Floating)'
Ui-StartApp 'view-show-kinds' @('--chrome=floating', '--open', $components)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'View Show menu: Origins off | Construction on | Sketches on' 'View > Show''s entries, Origins unchecked while the Origin folder''s light bulb is off (mitcad#1642)'
Ui-ExpectLog 'Construction shown: ' 'the planes drawn'
Expect-Planes 'both components'' planes drawn'
Ui-ExpectLog 'Sketches shown: F2, F4' 'both components'' sketches drawn'
Ui-ShowOrigin

Write-Host '--- Construction'
Ui-Mark
Ui-Step 'View > Show > Construction off (F8)' { Ui-Key 'F8' }
Ui-ExpectNew 'View show construction: hidden' 'Construction turned off'
Ui-ExpectNew 'Construction shown: none' 'no plane drawn in either component'
Ui-ExpectNew 'View Show menu: Origins on | Construction off | Sketches on' 'its check mark off'
if (Last-New 'Sketches shown: ') { Ui-Fail 'the sketches changed' }
$origin = "$(Ui-LastLineWith 'Design origin ' -Start)"
if ($origin -ne 'Design origin shown in the view; the Origin folder''s light bulb on') { Ui-Fail "the origin changed: $origin" }
Write-Host 'ok   the sketches and the origin stay'
$colours = "$(Ui-LastLineWith 'Browser row colours: ' -Start)"
if (-not ($colours.Contains('Frame:1/Construction/FramePlane') -and $colours.Contains('Motor:1/Construction/MotorPlane'))) {
  Ui-Fail "the planes' rows are not greyed: $colours"
}
Write-Host 'ok   the planes'' rows greyed'
Ui-Step 'save (Ctrl+S)'                    { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$saved = Get-Content -Raw $components | ConvertFrom-Json
if (-not $saved.display -or -not $saved.display.hidden -or $saved.display.hidden.construction -ne $true -or
    $saved.display.hidden.origins -or $saved.display.hidden.sketches) {
  Ui-Fail "the design file's display.hidden is not Construction only: $($saved.display | ConvertTo-Json -Compress)"
}
$lit = @($saved.features | Where-Object { $_.PSObject.Properties.Name -contains 'visible' } | ForEach-Object { $_.name })
if ($lit.Count -gt 0) { Ui-Fail "light bulbs set: $($lit -join ', ')" }
Write-Host 'ok   the design keeps Construction hidden as display state, no light bulb set'
Ui-Mark
Ui-Step 'View > Show > Construction on (F8)' { Ui-Key 'F8' }
Ui-ExpectNew 'View show construction: shown' 'Construction turned on'
Ui-ExpectNew 'Construction shown: ' 'the planes drawn again'
Expect-Planes 'both components'' planes drawn again'
Ui-ExpectNew 'View Show menu: Origins on | Construction on | Sketches on' 'its check mark on'
$colours = "$(Ui-LastLineWith 'Browser row colours: ' -Start)"
if ($colours.Contains('/Construction/')) { Ui-Fail "a plane's row is still greyed: $colours" }
Write-Host 'ok   the planes'' rows no longer greyed'

Write-Host '--- Sketches and Origins'
Ui-Mark
Ui-Step 'View > Show > Sketches off (F9)'  { Ui-Key 'F9' }
Ui-ExpectNew 'Sketches shown: none' 'no sketch drawn'
Ui-Mark
Ui-Step 'View > Show > Sketches on (F9)'   { Ui-Key 'F9' }
Ui-ExpectNew 'Sketches shown: F2, F4' 'both sketches drawn again'
Ui-Mark
Ui-Step 'View > Show > Origins off (F7)'   { Ui-Key 'F7' }
Ui-ExpectNew 'Design origin hidden in the view; the Origin folder''s light bulb on' 'the origin hidden, its light bulb still on'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Hide All Origins' 'an undo step of its own'
Ui-ExpectNew 'Design origin shown in the view; the Origin folder''s light bulb on' 'the origin back'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows View Show test'
