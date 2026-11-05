# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/RememberedValues.cpp tools/cli
# check-all sources: app/commands/HoleCommands.cpp
# Input-driven Windows UI test of the remembered values of Fillet and
# Chamfer (mitcad#1028) in the build VM (tools/ui-windows-lib.ps1 says how
# the input is posted), on a design mitcad-cli makes: a block 60 x 40 x 20
# (Box1, F1, Body1). The test library starts panels from their defaults
# (MITCAD_REMEMBER_PANEL_VALUES=0); this test turns the remembered values
# on, in settings of its own:
#   - Fillet starts at 2 mm; the top face's edges filleted with 3 mm
#     typed, OK; Fillet again starts with 3 mm (logged "Panel Fillet
#     radius: remembered 3 mm"); 5 mm typed and cancelled changes nothing.
#   - The fillet undone; Chamfer starts at 1 mm; the top face's edges
#     bevelled with 3 mm typed, OK; Chamfer again starts with 3 mm.
#   - A new start of the app: Fillet still starts with 3 mm.
#   - Thread's settings kept by size (mitcad#1298), on a block with two
#     8.5 mm holes and a 6.8 mm one: the first 8.5 mm hole threaded M10x1.5
#     6H (suggested), Left Hand, Modeled, not Full Length, 12 mm long, OK;
#     Thread on the second 8.5 mm hole starts from those settings and
#     makes the same thread (the same volume taken out); on the 6.8 mm
#     hole the size is suggested (M8x1.25 6H) and the rest are the
#     defaults (cosmetic).
# The Linux counterparts are the remembered steps of
# tools/ui-fillet-handle-test.sh and tools/ui-chamfer-handle-test.sh, and
# tools/ui-thread-settings-test.sh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-remembered-values-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-remembered-values'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.chamfer' = 'F7'; 'solid.thread' = 'F8' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'remembered.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 40, "height": 20, "operation": "new_body"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Remembered values on for this test (the library leaves a set value).
$env:MITCAD_REMEMBER_PANEL_VALUES = '1'
Ui-StartApp 'remembered' @('--open', $design)
Ui-ExpectLog 'remembered.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the block shown'

Write-Host '--- Fillet with 3 mm'
Ui-Mark
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Fillet Radius: 2 mm = 2 mm' 'nothing remembered yet: the default 2 mm' 30
if ((Ui-LogLines) | Where-Object { $_.Contains('Panel Fillet radius: remembered') }) {
  Ui-Fail 'a radius remembered before any OK'
}
Ui-Step 'pick the top face'              { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-ExpectNew 'Fillet Edges: 1 face' 'the top face picked'
Ui-Step 'radius 3'                       { Ui-TypeIn 'Panel Fillet input sets.0.radius' '3' }
Ui-ExpectNew 'Fillet Radius: 3 = 3 mm' '3 mm typed'
Ui-ExpectNew 'Preview Fillet: ok' 'previewed' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added fillet on 4 edge(s)' "fillet on the face's 4 edges" 30
Ui-WaitIdle

Write-Host '--- Fillet again starts with 3 mm; a cancelled 5 mm changes nothing'
Ui-Mark
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Panel Fillet radius: remembered 3 mm' 'the radius of the last OK remembered' 30
Ui-ExpectNew 'Fillet Radius: 3 mm = 3 mm' "the panel's Radius is 3 mm"
Ui-Step 'radius 5'                       { Ui-TypeIn 'Panel Fillet input sets.0.radius' '5' }
Ui-ExpectNew 'Fillet Radius: 5 = 5 mm' '5 mm typed'
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Fillet cancelled' 'the fillet cancelled' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'undo the fillet (Ctrl+Z)'       { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Fillet1' 'back to the block' 30
Ui-WaitIdle

Write-Host '--- Chamfer with 3 mm'
Ui-Mark
Ui-Step 'chamfer (F7)'                   { Ui-Key 'F7' }
Ui-ExpectNew 'Chamfer Distance: 1 mm = 1 mm' 'nothing remembered yet: the default 1 mm' 30
Ui-Step 'pick the top face'              { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-ExpectNew 'Chamfer Edges: 1 face' 'the top face picked'
Ui-Step 'distance 3'                     { Ui-TypeIn 'Panel Chamfer input sets.0.distance' '3' }
Ui-ExpectNew 'Chamfer Distance: 3 = 3 mm' '3 mm typed'
Ui-ExpectNew 'Preview Chamfer: ok' 'previewed' 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added chamfer (Equal Distance) on 4 edge(s)' "chamfer on the face's 4 edges" 30
Ui-WaitIdle
Ui-Mark
Ui-Step 'chamfer (F7)'                   { Ui-Key 'F7' }
Ui-ExpectNew 'Panel Chamfer distance: remembered 3 mm' 'the distance of the last OK remembered' 30
Ui-ExpectNew 'Chamfer Distance: 3 mm = 3 mm' "the panel's Distance is 3 mm"
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Chamfer cancelled' 'the chamfer cancelled' 30
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-StopApp

Write-Host '--- A new start of the app: Fillet still starts with 3 mm'
Ui-StartApp 'again' @('--open', $design)
Ui-ExpectLog 'remembered.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Panel Fillet radius: remembered 3 mm' 'the cancelled 5 mm did not change it' 30
Ui-ExpectNew 'Fillet Radius: 3 mm = 3 mm' "the panel's Radius is 3 mm"
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Fillet cancelled' 'the fillet cancelled' 30
Ui-StopApp

Write-Host '--- Thread starts from the last thread of the same size (mitcad#1298)'
# A block 40 x 20 x 16 (F1, F2: Body1, F2.b0) with 8.5 mm holes at (-12, 0)
# (F4) and (0, 0) (F6), the tap drill of M10x1.5, and a 6.8 mm one at
# (12, 0) (F8), M8x1.25's.
$holesScript = Join-Path $work 'holes.json'
$holes = Join-Path $work 'holes.mitcad'
Set-Content -Encoding ascii $holesScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-20, -10], "width": 40, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 16}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [-12, 0], "diameter": 8.5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [0, 0], "diameter": 8.5},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F7", "center": [12, 0], "diameter": 6.8},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}}
]
'@
$made = & $cli run $holesScript --save $holes
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $holes)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
$holeA = 'F4:side(c1)'
$holeB = 'F6:side(c1)'
$holeC = 'F8:side(c1)'
# (The diameter sign spelled out: the script stays ASCII.)
$d8 = "$([char]0x00D8)8.5 internal"
$d6 = "$([char]0x00D8)6.8 internal"

# The volume the last feature since the mark took out of the block.
function Removed-Volume([string]$Description) {
  $regex = New-Object System.Text.RegularExpressions.Regex 'Body Body1 \(F2\.b0\): volume ([0-9.]+) -> ([0-9.]+) mm3'
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ($clock.ElapsedMilliseconds -lt 30000) {
    $lines = @(Ui-NewLines)
    for ($i = $lines.Count - 1; $i -ge 0; $i--) {
      $match = $regex.Match($lines[$i])
      if ($match.Success) {
        $culture = [Globalization.CultureInfo]::InvariantCulture
        return [double]::Parse($match.Groups[1].Value, $culture) - [double]::Parse($match.Groups[2].Value, $culture)
      }
    }
    Start-Sleep -Milliseconds 100
  }
  Ui-Fail "${Description}: no volume logged"
}

Ui-StartApp 'thread' @('--open', $holes)
Ui-ExpectLog 'holes.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'thread (F8)'                    { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Thread choices: standard=iso_metric, size=10, designation=M10x1.5, class=6g, direction=right' 'the panel starts from the defaults' 30
Ui-ExpectNew "Pick face F2.b0/$holeA at" 'the first hole can be picked'
Ui-Mark
Ui-Step 'pick the first 8.5 mm hole'     { Ui-ClickPick "face F2.b0/$holeA" }
Ui-ExpectNew "Thread Faces: 1 face [face $holeA of F2.b0" 'the hole picked'
Ui-ExpectNew 'Preview Thread: ok' 'M10x1.5 6H suggested and previewed' 30
if (Ui-NewLines | Where-Object { $_.Contains('settings of the last') }) { Ui-Fail 'settings compared before any OK' }
Ui-Mark
Ui-Step 'Direction: Left Hand (wheel)'   { Ui-WheelLogged 'Panel Thread input direction' -1 }
Ui-ExpectNew 'Thread: Direction = Left Hand' 'left-handed'
Ui-Step 'Modeled'                        { Ui-ClickLogged 'Panel Thread input modeled' }
Ui-ExpectNew 'Thread: Modeled = on' 'modelled'
Ui-Step 'Full Length off'                { Ui-ClickLogged 'Panel Thread input full_length' }
Ui-ExpectNew 'Thread: Full Length = off' 'not the full length'
Ui-Step 'length 12'                      { Ui-TypeIn 'Panel Thread input length' '12' }
Ui-ExpectNew 'Thread Length: 12 = 12 mm' '12 mm long'
Ui-ExpectNew 'Preview Thread: ok' 'the modelled thread previewed' 180
Ui-WaitIdle
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added modeled thread M10x1.5 6H on 1 face(s)' 'M10x1.5 6H modelled in the first hole' 180
Ui-ExpectNew "Panel Thread: settings kept with $d8" "its settings kept with the hole's size"
$first = Removed-Volume 'the first thread'
Write-Host "ok   the first thread took $first mm3 out of the block"
Ui-WaitIdle

Ui-Mark
Ui-Step 'thread (F8)'                    { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Thread choices: standard=iso_metric, size=10, designation=M10x1.5, class=6g, direction=right' 'the defaults before a face is known' 30
Ui-ExpectNew "Pick face F2.b0/$holeB at" 'the second hole can be picked'
Ui-Mark
Ui-Step 'pick the second 8.5 mm hole'    { Ui-ClickPick "face F2.b0/$holeB" }
Ui-ExpectNew "Thread Faces: 1 face [face $holeB of F2.b0" 'the hole picked'
Ui-ExpectNew "Panel Thread: settings of the last $d8 thread restored" "the last thread's settings restored"
Ui-ExpectNew 'Panel Thread restored: ' 'what was restored logged'
$restored = ("$(Ui-LastLineWith 'Panel Thread restored: ')" -split 'restored: ', 2)[1]
foreach ($setting in @('direction=left', 'modeled=on', 'full_length=off', 'length=12 mm')) {
  if (-not ", $restored,".Contains(", $setting,")) { Ui-Fail "$setting not restored: $restored" }
}
Write-Host "ok   Left Hand, Modeled, not Full Length, 12 mm restored: $restored"
Ui-ExpectNew 'Thread Length: 12 mm = 12 mm' "the panel's Length is 12 mm"
Ui-ExpectNew 'Preview Thread: ok' 'the restored thread previewed' 180
Ui-WaitIdle
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added modeled thread M10x1.5 6H on 1 face(s)' 'M10x1.5 6H modelled in the second hole' 180
$second = Removed-Volume 'the second thread'
if ([Math]::Pow($first - $second, 2) -ge 1e-6) { Ui-Fail "the second thread took $second mm3, the first $first mm3" }
Write-Host "ok   the same thread: $second mm3 taken out, as by the first"
Ui-WaitIdle

Ui-Mark
Ui-Step 'thread (F8)'                    { Ui-Key 'F8' }
Ui-ExpectNew 'Command Thread started' 'Thread started' 30
Ui-ExpectNew "Pick face F2.b0/$holeC at" 'the 6.8 mm hole can be picked'
Ui-Mark
Ui-Step 'pick the 6.8 mm hole'           { Ui-ClickPick "face F2.b0/$holeC" }
Ui-ExpectNew "Thread Faces: 1 face [face $holeC of F2.b0" 'the hole picked'
Ui-ExpectNew "Panel Thread: settings of the last $d8 thread not used: $d6 differs" 'the 8.5 mm settings not used'
if (Ui-NewLines | Where-Object { $_.Contains('Panel Thread restored') }) { Ui-Fail 'settings restored on the 6.8 mm hole' }
Ui-ExpectNew 'Preview Thread: ok' 'previewed' 30
Ui-WaitIdle
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added cosmetic thread M8x1.25 6H on 1 face(s)' 'M8x1.25 6H suggested, cosmetic by default' 60
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Remove-Item Env:MITCAD_REMEMBER_PANEL_VALUES -ErrorAction SilentlyContinue

Ui-Finish 'UI Windows remembered values test'
