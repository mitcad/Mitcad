# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowSelectionCreate.cpp app/framework/SelectionCreate.cpp
# check-all sources: app/framework/SelectionCreate.hpp app/framework/WindowStyle.cpp app/MainWindow.cpp
# check-all sources: app/framework/Command.cpp app/framework/CommandSession.cpp app/commands tools/cli
# check-all sources: app/MainWindowMake.cpp app/print/MakeBodyCard.cpp
# check-all sources: core/model/src/features/thread.rs
# Input-driven Windows UI test of the selection card's Create row
# (mitcad#619) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), in the Docked layout, as tools/ui-selection-create-test.sh
# on Linux (which also runs it in the Floating layout), on a design
# mitcad-cli makes: a cylinder 20 mm across and 30 high at the origin
# (Extrude1, F2.b0, its round face F2:side(c1)) and an unused 20 mm square
# beside it (Sketch2, F3):
#   1. The square's profile selected: the row offers Extrude, Revolve and
#      Sweep ("Selection card create: Extrude | Revolve | Sweep").
#   2. Its Extrude button clicked: Extrude starts with that profile in
#      Profiles. Esc cancels.
#   3. The cylinder's round face selected: Thread first.
#   3b. The Make tab (mitcad#1178): no Create row ("Selection card create:
#      none"), the Make section of the face's body in its place, inside the
#      selection card with its frame whole and the same margin on both
#      sides (the design has a process of a long name, which made the
#      section wider than the card). Back on the Solid tab the row is
#      Thread's again, inside the card too.
#   4. The selection cleared (Esc): "Selection card create: none".
#   4b. Thread on the round face with a size far from it (mitcad#1262):
#      two sizes down from the suggested M20x2.5, M24x3, previews with the
#      diameter warning in its log line and the panel's message
#      (tools/ui-create-test.sh checks the same of a hole on Linux).
#      Cancelled.
#   5. A custom thread (mitcad#893) on the round face: Thread Type Custom
#      with a typed major diameter (20) and pitch (3, which no ISO metric
#      thread of 20 mm has), Starts 2, modelled, without the printed-fit
#      clearances a new thread starts with. Its profile is the basic
#      M10x1.5's at twice the size, and two starts leave a single-start
#      thread's volume: four times the modelled M10x1.5's per millimetre
#      (tools/ui-create-test.sh, thread_modeled: 1302.830611500115 mm3 over
#      20 mm), over 30 mm.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-selection-create-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-selection-create'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Thread by a shortcut of the test's own: session 0 has no command search.
Ui-Init $App $Out $QtBin @{ 'solid.thread' = 'F7' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$side = 'F2:side(c1)'

$modelJson = Join-Path $work 'model.json'
$design = Join-Path $work 'create.mitcad'
Set-Content -Encoding ascii $modelJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [30, -10], "width": 20, "height": 20},
  {"cmd": "create_print_profile", "kind": "process",
    "name": "Fine detail 0.12 mm layers with extra walls and a slow outer wall", "values": {"layer_height": 0.12}}
]
'@
$output = & $cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# Expect-Create "A | B" "description": the row's log line is exactly that,
# since the mark (up to 6 s).
function Expect-Create([string]$Text, [string]$Description) {
  for ($i = 0; $i -lt 30; $i++) {
    $lines = Ui-LogLines | Select-Object -Skip $script:UiMark
    if ($lines | Where-Object { $_ -ceq "Selection card create: $Text" }) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  $last = Ui-LogLines | Where-Object { $_.StartsWith('Selection card create: ') } | Select-Object -Last 1
  Ui-Fail "${Description}: the row is not '$Text' ($last)"
}

# Section-Fits "Make": the selection card's section as last logged
# (mitcad#1178): inside the card and what of it shows, its right frame
# whole, with the same margin on both sides unless the page scrolls.
function Section-Fits([string]$Name) {
  $line = "$(Ui-LastLineWith "Selection card section ${Name}: ")"
  $m = [regex]::Match($line, ': (-?\d+)\.\.(-?\d+) of card (-?\d+)\.\.(-?\d+), visible to (-?\d+)')
  if (-not $m.Success) { Ui-Fail "no place logged for the $Name section ($line)" }
  $l = [int]$m.Groups[1].Value; $r = [int]$m.Groups[2].Value
  $cl = [int]$m.Groups[3].Value; $cr = [int]$m.Groups[4].Value; $vr = [int]$m.Groups[5].Value
  if ($l -lt $cl -or $r -gt $cr -or $r -gt $vr) { Ui-Fail "the $Name section runs past the card: $line" }
  if (-not $line.EndsWith(', scrolled') -and [Math]::Abs(($l - $cl) - ($cr - $r)) -gt 1) {
    Ui-Fail "the $Name section's margins differ (left $($l - $cl), right $($cr - $r)): $line"
  }
  Write-Host "ok   the $Name section inside the card ($l..$r of $cl..$cr, visible to $vr)"
}

Ui-StartApp 'selection-create' @('--open', $design)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cylinder shown'

# Extrude once for the log's pick places of the profiles: it takes the
# square, the newest profile. Cancelled.
Ui-Mark
Ui-Step 'extrude (E)'                      { Ui-Key 'e' }
Ui-ExpectNew "Extrude Profiles: 1 profile [profile $rect of F3]" 'with nothing selected, the square'
Ui-ExpectNew "Pick profile F3/$rect at" 'the square can be picked'
Ui-Step 'cancel (Esc)'                     { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'cancelled'

Write-Host '--- 1. A profile selected'
Ui-Mark
Ui-Step 'select the square'                { Ui-ClickPick "profile F3/$rect" }
Ui-ExpectNew "Selected: 1 profile [profile $rect of F3]" 'the square selected'
Expect-Create 'Extrude | Revolve | Sweep' 'the row: Extrude, Revolve, Sweep'
Ui-ExpectNew 'Selection card create Extrude at ' 'its Extrude button laid out'

Write-Host '--- 2. Its Extrude button'
Ui-Mark
Ui-Step 'click Extrude on the card'        { Ui-ClickLogged 'Selection card create Extrude' }
Ui-ExpectNew 'Selection card create chose Extrude' 'the card''s button clicked'
Ui-ExpectNew 'Command Extrude started' 'Extrude started'
Ui-ExpectNew "Extrude Profiles: 1 profile [profile $rect of F3]" 'with the selected profile'
Ui-ExpectNew 'Preview Extrude: ok' 'previewed' 30
Ui-Mark
Ui-Step 'cancel (Esc)'                     { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'cancelled'

Write-Host '--- 3. A cylinder''s round face selected'
# Thread once for the log's pick places of the round face. Cancelled.
Ui-Mark
Ui-Step 'thread (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Thread started' 'Thread started'
Ui-ExpectNew "Pick face F2.b0/$side at" 'the round face can be picked'
Ui-Step 'cancel (Esc)'                     { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Thread cancelled' 'cancelled'
Ui-Mark
Ui-Step 'select the round face'            { Ui-ClickPick "face F2.b0/$side" }
Ui-ExpectNew "Selected: 1 face [face $side of F2.b0" 'the round face selected'
Ui-ExpectNew 'Selection card create: Thread' 'Thread first'
$last = Ui-LogLines | Where-Object { $_.StartsWith('Selection card create: ') } | Select-Object -Last 1
if ("$last".Contains('Extrude')) { Ui-Fail "Extrude offered for a round face: $last" }
Write-Host 'ok   no Extrude for the round face'
Ui-ExpectNew 'Selection card section Create: ' 'the Create row''s place logged'
Section-Fits 'Create'

Write-Host '--- 3b. The Make tab: no Create row, the Make section in its place'
Ui-Mark
Ui-Step 'the Make tab'                     { Ui-ClickLogged 'Ribbon tab MAKE' }
Ui-ExpectNew 'Ribbon tab shown: MAKE' 'the Make tab shown'
Expect-Create 'none' 'no Create row on the Make tab'
Ui-ExpectNew 'Make card: Body1: ' 'the Make section of the face''s body'
Ui-ExpectNew 'Selection card section Make: ' 'its place logged'
Section-Fits 'Make'
if (Ui-NewLines | Where-Object { $_.Contains('Selection card section Create: ') }) {
  Ui-Fail 'the Create row laid out on the Make tab'
}
Write-Host 'ok   no Create row laid out on the Make tab'
Ui-Mark
Ui-Step 'the Solid tab'                    { Ui-ClickLogged 'Ribbon tab SOLID' }
Ui-ExpectNew 'Ribbon tab shown: SOLID' 'the Solid tab shown'
Ui-ExpectNew 'Selection card create: Thread' 'the Create row back, Thread first'
Ui-ExpectNew 'Make card: none' 'no Make section on the Solid tab'
Ui-ExpectNew 'Selection card section Create: ' 'the Create row''s place logged'
Section-Fits 'Create'

Write-Host '--- 4. The selection cleared'
Ui-Mark
Ui-Step 'clear the selection (Esc)'        { Ui-Key 'Escape' }
Ui-ExpectNew 'Selected: nothing' 'nothing selected'
Expect-Create 'none' 'the row: none'

Write-Host '--- 4b. A size far from the face (mitcad#1262)'
# The face suggests M20x2.5; two wheel steps down the sizes, M24x3, whose
# major diameter is 4 mm larger than the face's: the preview and the
# panel's message warn, the thread previewed all the same. Cancelled.
Ui-Mark
Ui-Step 'thread (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Thread started' 'Thread started'
Ui-Step 'pick the round face'              { Ui-ClickPick "face F2.b0/$side" }
Ui-ExpectNew "Thread Faces: 1 face [face $side of F2.b0" 'the round face picked'
Ui-ExpectNew 'Preview Thread: ok' 'the suggested size previewed' 30
Ui-Mark
Ui-Step 'Size: two down (24)'              {
  for ($i = 0; $i -lt 2; $i++) {
    Ui-WheelLogged 'Panel Thread input size' -1
    Ui-Sync
  }
}
Ui-ExpectNew 'Thread: Size = 24' 'size 24 chosen'
# (The diameter sign by its code: the script is read in the system's code
# page.)
$dia = [string][char]0x00D8
# The preview's log line carries the model's warning, which names the
# face; the panel's notice names none, its place is in the notice
# (mitcad#1327).
$m24 = "M24x3 (${dia}24 mm) on face $side (${dia}20 mm): the face's diameter is 4 mm smaller than the thread's major diameter"
$m24Notice = "M24x3 (${dia}24 mm) on a face (${dia}20 mm): the face's diameter is 4 mm smaller than the thread's major diameter"
Ui-ExpectNew "Preview Thread: ok, warning: $m24" 'the preview warns of the size' 30
Ui-ExpectNew "Panel Thread message box: warning: $m24Notice" 'the panel''s message shows the warning'
Ui-Mark
Ui-Step 'cancel (Esc)'                     { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Thread cancelled' 'cancelled'

Write-Host '--- 5. A custom thread on two starts (mitcad#893)'
Ui-Mark
Ui-Step 'thread (F7)'                      { Ui-Key 'F7' }
Ui-ExpectNew 'Command Thread started' 'Thread started'
Ui-Step 'pick the round face'              { Ui-ClickPick "face F2.b0/$side" }
Ui-ExpectNew "Thread Faces: 1 face [face $side of F2.b0" 'the round face picked'
Ui-Mark
# Thread Type's drop-down cannot open in session 0: eight wheel steps down
# to Custom, the last.
Ui-Step 'Thread Type: Custom (wheel)'      {
  for ($i = 0; $i -lt 8; $i++) {
    Ui-WheelLogged 'Panel Thread input standard' -1
    Ui-Sync
  }
}
Ui-ExpectNew 'Thread: Thread Type = Custom' 'a custom thread'
Ui-Step 'major diameter 20'                { Ui-TypeIn 'Panel Thread input thread_major' '20' }
Ui-Step 'pitch 3'                          { Ui-TypeIn 'Panel Thread input thread_pitch' '3' }
Ui-Mark
Ui-Step 'Modeled'                          { Ui-ClickLogged 'Panel Thread input modeled' }
Ui-ExpectNew 'Thread: Modeled = on' 'modelled'
Ui-Step 'Starts 2'                         { Ui-TypeIn 'Panel Thread input starts' '2' }
# A new modelled thread starts with the printed-fit clearances; without
# them it is the plain profile the volume below is of.
Ui-Step 'radial clearance 0'               { Ui-TypeIn 'Panel Thread input radial_clearance' '0' }
Ui-Step 'flank clearance 0'                { Ui-TypeIn 'Panel Thread input flank_clearance' '0' }
Ui-ExpectNew 'Preview Thread: ok' 'the two-start thread previewed' 120
if (Ui-NewLines | Where-Object { $_.Contains('Preview Thread: failed') }) { Ui-Fail 'the custom thread was refused' }
Ui-FocusMain
Ui-Mark
Ui-Step 'OK (Enter)'                       { Ui-Key 'Return' }
Ui-ExpectNew 'Added modeled thread custom 20x3, 2 starts on 1 face(s)' 'custom 20 x 3 on two starts, modelled' 60
Ui-ExpectVolume 'Body Body1 \(F2\.b0\): volume 9424\.778 -> ([0-9.]+) mm3' 7816.98366900069 `
  'two starts of the 20 x 3 profile: four times the modelled M10x1.5 per millimetre' 1e-3

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows selection create test'
