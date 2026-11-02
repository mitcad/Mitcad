# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework app/MainWindow.cpp app/MainWindowVisibility.cpp tools/cli app/OcctViewer.cpp
# Input-driven Windows UI test of the browser's light bulbs in every mode
# (mitcad#295) and Create Sketch's origin planes (mitcad#844) in the build
# VM (tools/ui-windows-lib.ps1 says how the input is posted); the Linux test
# is tools/ui-visibility-test.sh:
#   1. A design without components (mitcad-cli): Body1 (20 x 20 x 10)
#      inside Body2 (60 x 60 x 30), Sketch3 with a free 20 x 20 square
#      beside them. Create Sketch shows the origin planes, no face of Body1
#      can be picked; Body2 hidden by its light bulb while the plane is
#      asked for, Body1's top face can be picked and the sketch starts on
#      it. Transparent (mitcad#1093) on Body2 from its Browser row: the row
#      shows the half-open eye with its name in no colour (mitcad#1143), a
#      click where Body1 lies inside Body2 selects a face of Body1, the menu
#      has Transparent checked; one undo step makes Body2 opaque, and the
#      click takes Body2 again.
#   2. Extrude of the square, Body1 hidden meanwhile: the preview drawn
#      again without Body1, not computed again; Cancel keeps Body1 hidden
#      (the change is the last undo step).
#   3. Extrude again, Body1 hidden meanwhile, OK: one undo step takes the
#      extrusion back and leaves Body1 hidden; the next one shows it.
#   4. Edit of Extrude2 (the timeline rolled back), Body1 hidden meanwhile,
#      Cancel: Body1 stays hidden, the last undo step (no marker move left).
#      Then the Origin folder's light bulb off: Offset Plane's plane input
#      and Extrude's Start Object leave the origin hidden (mitcad#1030), and
#      XY picked from the browser's Origin folder goes to the input; with
#      the light bulb on, the origin stays shown during and after Offset
#      Plane, and XY is picked in the view.
#   5. The same design with a component (Part, its body across the origin)
#      and the root active: Create Sketch shows the origin planes too, the
#      Origin folder's light bulb off and unchanged, behind the bodies for
#      the pick: a click on Part's top face, which XZ and YZ run through
#      and XY lies below, picks the face; the component's radio button
#      (another browser action) says "Finish the command or the sketch
#      first." during the pick and during Extrude, where the component's
#      light bulb works.
#   6. The same design: a click on XZ beside the bodies picks the plane, and
#      after the pick the origin is hidden again, as its light bulb is off;
#      the Origin folder's light bulb switched on during the pick: XY can
#      be picked, and the origin stays shown in the sketch.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-visibility-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-visibility'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'construct.offset_plane' = 'F8' } -StepPause 0

# The view's origin and the Origin folder's light bulb (mitcad#739).
$originShown = "Design origin shown in the view; the Origin folder's light bulb off"
$originHidden = "Design origin hidden in the view; the Origin folder's light bulb off"
$originOn = "Design origin shown in the view; the Origin folder's light bulb on"

# Ui-ExpectNone "text" "description": no line since the mark has the text.
function Ui-ExpectNone([string]$Text, [string]$Description) {
  Ui-Sync
  Start-Sleep -Milliseconds 500
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains($Text) }) {
    Ui-Fail "${Description}: '$Text' is in the log"
  }
  Write-Host "ok   $Description"
}

# Menu-Choose "entry": the entry of the context menu logged last ("Context
# menu: A | B"), chosen with the keys as the menu opens with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ')
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
  Ui-Key 'Return'
}

# Last-Line "text": the last line logged that has the text.
function Last-Line([string]$Text) {
  return "$(Ui-LogLines | Where-Object { $_.Contains($Text) } | Select-Object -Last 1)"
}

# Extrude, which takes the newest free profile (the square): its preview
# computed with both blocks drawn.
function Extrude-Square {
  Ui-Mark
  Ui-Step 'extrude (E)'                  { Ui-Key 'e' }
  Ui-ExpectNew 'Command Extrude started' 'Extrude started'
  Ui-ExpectNew 'Preview Extrude: ok' 'the square''s preview computed'
  Ui-ExpectNew 'Preview bodies: ' 'the preview drawn'
  if (-not (Last-Line 'Preview bodies: ').Contains('F2.b0 at')) { Ui-Fail 'the preview does not draw Body1' }
  Write-Host 'ok   the preview draws Body1'
}

# Body1's light bulb during a command: the preview drawn again without it,
# not computed again.
function Hide-Body1([string]$Command) {
  Ui-Mark
  Ui-Step 'hide Body1'                   { Ui-ClickLogged 'Browser eye Root/Bodies/Body1' }
  Ui-ExpectNew "Visibility Root/Bodies/Body1: hidden (during $Command)" "Body1 hidden during $Command"
  Ui-ExpectNew 'Preview bodies: ' 'the preview drawn again'
  $drawn = Last-Line 'Preview bodies: '
  if ($drawn.Contains('F2.b0 at')) { Ui-Fail "the preview still draws Body1: $drawn" }
  if (-not $drawn.Contains('F4.b0 at')) { Ui-Fail "the preview lost Body2: $drawn" }
  Write-Host 'ok   the preview without Body1, with Body2'
  Ui-ExpectNone "Preview ${Command}: " 'the preview not computed again'
}

# Place-Valid "text": whether the last line about it gives its place
# ("<text> at x,y") and not "<text> out of sight" (BrowserPanel::logLayout).
function Place-Valid([string]$Text) {
  $last = [string](Ui-LogLines | Where-Object { $_.Contains("$Text at ") -or $_.Contains("$Text out of sight") } |
    Select-Object -Last 1)
  return ($last -ne '' -and -not $last.EndsWith(' out of sight'))
}

# Click-Row "Browser <path>" (or "Browser eye|radio <path>"): a browser row
# clicked, scrolled into sight first by wheel steps over the rows (down, then
# up) when it is not: rows below the card's edge in session 0's 1024x768
# desktop are scrolled to; a place the row has left is never clicked.
function Click-Row([string]$Text) {
  foreach ($steps in @(-2, 2)) {
    for ($i = 0; $i -lt 8 -and -not (Place-Valid $Text); $i++) {
      $row = Ui-LogLines | Select-String -Pattern '^Browser (?:eye |radio )?Root.* at (\d+),(\d+)$' |
        Select-Object -Last 1
      if (-not $row) { break }
      $x = [int]$row.Matches[0].Groups[1].Value
      $y = [int]$row.Matches[0].Groups[2].Value
      [UiInput]::Move($script:UiWindow, $x, $y, 0)
      Start-Sleep -Milliseconds 80
      [UiInput]::Wheel($script:UiWindow, $x, $y, $steps)
      Ui-Sync
      Start-Sleep -Milliseconds 300
    }
  }
  if (-not (Place-Valid $Text)) { Ui-Fail "'$Text' is out of sight (scrolled away, folded or gone)" }
  Ui-ClickLogged $Text
}

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# Sketch1 (F1) 20 x 20 at (40, -60), Extrude1 (F2) 10 up: Body1 (F2.b0).
# Sketch2 (F3) 60 x 60 around it, Extrude2 (F4) 30 up: Body2 (F4.b0), Body1
# inside it. Sketch3 (F5) 20 x 20 at (100, -140): a profile in front of the
# blocks as the view looks at them, away from the origin planes.
$blocks = @'
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [40, -60], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [20, -80], "width": 60, "height": 60},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [100, -140], "width": 20, "height": 20}
'@
# The same with Part:1 (O1, C1) at the origin: Sketch4 (F6) 20 x 20 around
# its origin, Extrude3 (F7) 10 up: across XZ and YZ, on XY; the root active
# again.
$part = @'
  {"cmd": "create_component", "name": "Part", "transform": {"translation": [0, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [-10, -10], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F6", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
'@
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$design = Join-Path $work 'blocks.mitcad'
$parts = Join-Path $work 'parts.mitcad'
foreach ($made in @(@{ Script = "[`n$blocks`n]"; File = $design }, @{ Script = "[`n$blocks,`n$part`n]"; File = $parts })) {
  $script = [IO.Path]::ChangeExtension($made.File, '.json')
  Set-Content -Encoding ascii $script ($made.Script -creplace 'RECT', $rect)
  $output = & $cli run $script --save $made.File
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $made.File)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
}

Ui-StartApp 'visibility' @('--open', $design)
Ui-ExpectLog 'blocks.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0, F4.b0' 'both blocks shown'

Write-Host '--- Create Sketch: the origin planes; Body2 hidden while a plane is asked for'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: select a plane' 'Create Sketch asks for a plane'
Ui-ExpectNew 'Create Sketch: origin planes shown' 'no components: the origin planes shown'
Ui-ExpectNew 'Datum xy at' 'the XY plane can be picked'
Ui-ExpectNew 'Pick places:' 'the faces'' pick places logged'
Ui-ExpectNone 'Pick face F2.b0/' 'Body1, inside Body2, cannot be picked'
Ui-Mark
Ui-Step 'hide Body2'                     { Ui-ClickLogged 'Browser eye Root/Bodies/Body2' }
Ui-ExpectNew 'Visibility Root/Bodies/Body2: hidden (during Create Sketch)' 'Body2 hidden during the plane pick'
Ui-ExpectNew 'Pick face F2.b0/F2:end(' 'Body1''s top face can be picked now'
Ui-ExpectNone 'Create Sketch is waiting for a plane' 'the light bulb was not refused'
Ui-Step 'pick Body1''s top face'         { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Sketch started on {"body":"F2.b0","face":"F2:end(' 'the sketch started on Body1''s top face'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the empty sketch finished'
Ui-Mark
Ui-Step 'show Body2'                     { Ui-ClickLogged 'Browser eye Root/Bodies/Body2' }
Ui-ExpectNew 'Visibility Root/Bodies/Body2: shown' 'Body2 shown again'
Ui-ExpectNone '(during' 'no command runs'

Write-Host '--- Transparent (mitcad#1093): Body2 from its Browser row; a click goes through it to Body1'
Ui-Mark
Ui-Step 'right-click Body2''s row'       { Ui-ClickLogged 'Browser Root/Bodies/Body2' 3 }
Ui-ExpectNew 'Context menu: ' 'the row''s menu'
Ui-Step 'Transparent'                    { Menu-Choose 'Transparent' }
Ui-ExpectNew 'Transparent: Body2' 'Body2 made transparent'
Ui-ExpectNew 'Browser eye Root/Bodies/Body2: transparent' 'its row shows the half-open eye'
Ui-ExpectNew 'View transparent: 1 body' 'the view draws Body2 see-through'
# Its name keeps no colour (mitcad#1143: the transparency once read as a
# colour role and painted the name red, logged as "unknown 1: ...").
Ui-Sync
$colours = Last-Line 'Browser row colours: '
if (-not $colours) { Ui-Fail 'no ''Browser row colours'' line logged' }
if ($colours.Contains('Root/Bodies/Body2') -or $colours.Contains('unknown')) {
  Ui-Fail "the transparent row's name has a colour: $colours"
}
Write-Host 'ok   Body2''s name keeps the plain colour (no colour role on its row)'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'click where Body1 lies inside Body2' { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Selected: 1 face [face F2:' 'the click goes through Body2 to Body1 behind it'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'right-click Body2''s row'       { Ui-ClickLogged 'Browser Root/Bodies/Body2' 3 }
Ui-ExpectNew 'Context menu checked: Transparent' 'Transparent is checked on its row'
Ui-Step 'Escape (close the menu)'        { Ui-Key 'Escape' }
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Make Body2 Transparent' 'one undo step makes Body2 opaque again'
Ui-ExpectNew 'Browser eye Root/Bodies/Body2: opaque' 'its eye is open again'
Ui-ExpectNew 'View transparent: none' 'the view draws Body2 as before'
Ui-Mark
Ui-Step 'click there again'              { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Selected: 1 face [face F4:' 'opaque, Body2 takes the click'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- Extrude, Body1 hidden meanwhile, Cancel'
Extrude-Square
Hide-Body1 'Extrude'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'Extrude cancelled'
Ui-ExpectNone 'Bodies shown: F2.b0' 'Body1 stays hidden'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Hide Body1' 'the light bulb is the last undo step, kept by Cancel'
Ui-ExpectNew 'Bodies shown: F2.b0, F4.b0' 'Body1 shown by the undo'

Write-Host '--- Extrude, Body1 hidden meanwhile, OK: one undo step for the extrusion'
Extrude-Square
Hide-Body1 'Extrude'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added Extrude as F' 'Extrude3 added'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Add Extrude3' 'one undo step takes the extrusion back'
Ui-ExpectNone 'Bodies shown: F2.b0' 'Body1 stays hidden after that undo'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Hide Body1' 'the light bulb is an undo step of its own'
Ui-ExpectNew 'Bodies shown: F2.b0, F4.b0' 'Body1 shown by it'

Write-Host '--- Edit of Extrude2 (rolled back), Body1 hidden meanwhile, Cancel'
Ui-ExpectLog 'Timeline Extrude2 at' 'Extrude2 in the timeline'
Ui-Mark
Ui-Step 'edit Extrude2 (double-click)'   { Ui-DoubleClickLogged 'Timeline Extrude2' }
Ui-ExpectNew 'Editing F4 with Extrude' 'editing Extrude2'
Ui-ExpectNew 'Bodies shown: F2.b0' 'the timeline rolled back to before Body2'
Ui-Mark
Ui-Step 'hide Body1'                     { Ui-ClickLogged 'Browser eye Root/Bodies/Body1' }
Ui-ExpectNew 'Visibility Root/Bodies/Body1: hidden (during Extrude)' 'Body1 hidden during the edit'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude: 1 visibility change(s) kept' 'the change run again after the roll-back'
Ui-ExpectNew 'Command Extrude cancelled' 'the edit cancelled'
Ui-ExpectNew 'Bodies shown: F4.b0' 'Body2 back, Body1 hidden'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Hide Body1' 'the light bulb is the last undo step, no marker move left'
Ui-ExpectNew 'Bodies shown: F2.b0, F4.b0' 'Body1 shown by the undo'

Write-Host '--- Offset Plane and Extrude: a hidden origin stays hidden, picked from the browser (mitcad#1030)'
Ui-Mark
Ui-Step 'offset plane (F8)'              { Ui-Key 'F8' }
Ui-ExpectNew 'Command Offset Plane started' 'Offset Plane started'
Ui-ExpectNone $originShown 'the hidden origin stays hidden for the plane input'
Ui-ExpectNone 'Datum xy at' 'no origin plane in the view'
Ui-PickOriginRow 'XY'
Ui-ExpectNew 'Offset Plane Plane: 1 plane [plane xy]' 'XY picked from the browser while the origin is hidden'
Ui-ExpectNone $originShown 'the origin still hidden after the pick'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Offset Plane cancelled' 'Offset Plane cancelled'
Extrude-Square
Ui-Step 'Start: Offset (wheel)'          { Ui-WheelLogged 'Panel Extrude input start' -1 }
Ui-Step 'Start: Object (wheel)'          { Ui-WheelLogged 'Panel Extrude input start' -1 }
Ui-ExpectNew 'Extrude: Start = Object' 'the start is an object' 30
Ui-ExpectNone $originShown 'the hidden origin stays hidden for Start Object'
Ui-ExpectNone 'Datum xy at' 'no origin plane in the view'
Ui-PickOriginRow 'XY'
Ui-ExpectNew 'Extrude Start Object: 1 plane [plane xy]' 'XY picked from the browser as the start'
Ui-ExpectNone $originShown 'the origin still hidden after the pick'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'Extrude cancelled'

Write-Host '--- Offset Plane: a shown origin stays shown, picked in the view (mitcad#1030)'
Ui-Mark
Ui-Step 'show the origin'                { Click-Row 'Browser eye Root/Origin' }
Ui-ExpectNew 'Visibility Root/Origin: shown' 'the Origin folder''s light bulb on'
Ui-ExpectNew $originOn 'the origin shown'
Ui-Mark
Ui-Step 'offset plane (F8)'              { Ui-Key 'F8' }
Ui-ExpectNew 'Command Offset Plane started' 'Offset Plane started'
Ui-Step 'pick XY in the view'            { Ui-ClickLogged 'Datum xy' }
Ui-ExpectNew 'Offset Plane Plane: 1 plane [plane xy]' 'XY picked in the view'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Offset Plane cancelled' 'Offset Plane cancelled'
Ui-ExpectNone 'Design origin hidden in the view' 'the shown origin stayed shown during and after the command'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-StopApp

Write-Host '--- A design with a component: the origin planes shown, behind the bodies (mitcad#844)'
Ui-StartApp 'visibility-parts' @('--open', $parts)
Ui-ExpectLog 'parts.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser radio Root/Part:1 at' 'Part:1 in the browser'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: select a plane' 'Create Sketch asks for a plane'
Ui-ExpectNew 'Create Sketch: origin planes shown' 'a design with a component: the origin planes shown'
Ui-ExpectNew $originShown 'the design''s origin shown for the pick, its light bulb off'
Ui-ExpectNew 'Datum xy at' 'the XY plane can be picked'
Ui-ExpectNew 'Pick face F4.b0/F4:end(' 'Body2''s top face can be picked'
# Part's body lies across the origin: XZ and YZ run through it, XY lies
# below it, so a plane is in front of or behind each point of its top face.
Ui-ExpectNew 'Pick face F7.b0/F7:end(' 'Part''s top face, which the origin planes run through, can be picked'
Ui-Mark
Ui-Step 'activate Part (radio)'          { Click-Row 'Browser radio Root/Part:1' }
Ui-ExpectNew 'Create Sketch is waiting for a plane: pick one or press Esc.' 'other browser actions wait for the plane pick'
Ui-ExpectNone 'Activated Part' 'Part not activated'
Ui-Step 'pick Part''s top face'          { Ui-ClickPick 'face F7.b0/F7:end(' }
Ui-ExpectNew 'Sketch started on {"body":"F7.b0","face":"F7:end(' 'the face is picked, not the plane through it'
Ui-ExpectNew $originHidden 'the design''s origin hidden again in the sketch'
Ui-ExpectNone 'Visibility Root/Origin' 'the Origin''s light bulb not changed'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the empty sketch finished'

Write-Host '--- A click on an origin plane beside the bodies picks the plane; the light bulb as before'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: origin planes shown' 'the origin planes shown'
Ui-ExpectNew $originShown 'the design''s origin shown for the pick, its light bulb off'
Ui-ExpectNew 'Datum xz at' 'the XZ plane can be picked'
Ui-Step 'pick XZ in the view'            { Ui-ClickLogged 'Datum xz' }
Ui-ExpectNew 'Sketch started on xz' 'the sketch started on the origin''s XZ plane'
Ui-ExpectNew $originHidden 'the design''s origin hidden again: its light bulb is off'
Ui-ExpectNone 'Visibility Root/Origin' 'the Origin''s light bulb not changed'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the empty sketch finished'
Ui-ExpectNone $originShown 'the design''s origin stays hidden after the sketch'

Write-Host '--- The Origin folder''s light bulb during the pick: the origin stays shown after it'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: origin planes shown' 'the origin planes shown'
Ui-Mark
Ui-Step 'show the origin'                { Click-Row 'Browser eye Root/Origin' }
Ui-ExpectNew 'Visibility Root/Origin: shown (during Create Sketch)' 'the Origin folder shown during the pick'
Ui-ExpectNew $originOn 'its light bulb on'
Ui-Step 'pick XY'                        { Ui-ClickLogged 'Datum xy' }
Ui-ExpectNew 'Sketch started on xy' 'the sketch started on the origin''s XY plane'
Ui-Sync
$state = Ui-LastMatch '(Design origin (?:hidden|shown) in the view; .*)$'
if ($state -cne $originOn) { Ui-Fail "the origin is not shown in the sketch with its light bulb on: '$state'" }
Write-Host 'ok   the origin stays shown in the sketch, as its light bulb says'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the empty sketch finished'

Write-Host '--- During Extrude: a component''s light bulb works, other browser actions wait'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started'
Ui-ExpectNew 'Preview Extrude: ok' 'the square''s preview computed'
Ui-Mark
Ui-Step 'hide Part:1'                    { Ui-ClickLogged 'Browser eye Root/Part:1' }
Ui-ExpectNew 'Visibility Root/Part:1: hidden (during Extrude)' 'Part:1 hidden during Extrude'
Ui-Mark
Ui-Step 'activate Part (radio)'          { Click-Row 'Browser radio Root/Part:1' }
Ui-ExpectNew 'Finish the command or the sketch first.' 'other browser actions wait for the command'
Ui-ExpectNone 'Activated Part' 'Part not activated'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'Extrude cancelled'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows visibility test'
