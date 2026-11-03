# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.cpp app/commands/CommandSupport.cpp core/model/src/document.rs
# check-all sources: core/model-api/src/sketch.rs tools/cli
# check-all sources: app/framework/ParameterPicker.cpp app/framework/CommandSessionPicks.cpp
# check-all sources: app/framework/CommandPresenter.hpp app/MainWindowParameterPicks.cpp app/commands/FilletCommands.cpp
# Input-driven Windows UI test of dimension names (mitcad#231, mitcad#250)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted):
#   1. A design (mitcad-cli): a 40 x 25 rectangle with its width k5 (d1) and
#      height k6 (d2) dimensioned, its left side measured by the driven k7
#      (its reference parameter d3), extruded 10 (d4, F2), its corner at
#      (40, 0) rounded by Fillet1 (r 3, d5, F3), extruded -4 as a second
#      body (d6, F4); the sketch shown (its light bulb on).
#   2. Edit the sketch: the pointer over the height shows its tooltip; its
#      value editor shows "d2 =" and the tooltip text; a click on the width
#      inserts d1, and d1/2 makes the height follow.
#   3. "width = 50" in the width's editor renames d1 and sets it; "d2 = 30"
#      is refused (the name is taken) and the editor stays open.
#   4. The palette's Dimension text: Name, then Name = value (the driven
#      k7 with its reference d3).
#   5. Edit Extrude1: its distance field is labelled "Distance (d4)", its
#      tooltip says what d4 is (mitcad#250); "depth = 12" renames it.
#   6. Edit Extrude2 (mitcad#250): its field's tooltip; no labels while the
#      field has the keyboard only because the panel opened (mitcad#587); a
#      click into it labels the sketch's dimensions, and only those; Right
#      View, Front View and both again by keys (F7, F8), the field keeping
#      the keyboard: the labels stay and the width's place follows each
#      view, the same place for the same view (mitcad#1179); a
#      click on the width dimension inserts width, depth and d5 are typed
#      by name; the tooltip follows.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-dimension-names-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-dimension-names'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Right View and Front View by keys of the test's own (mitcad#1179).
Ui-Init $App $Out $QtBin @{ 'view.right' = 'F7'; 'view.front' = 'F8' } -StepPause 0

# The width's label's place for the camera now, once it has one since the
# mark: the last "Parameter label Sketch1 k5 width at x,y" after the mark.
# Session 0 paints no widgets, so this is the place the labels' layout
# gives, not one a paint drew (the Linux test checks those).
function Width-LabelFollows([string]$Description) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $at = $null
  do {
    foreach ($line in @(Ui-NewLines)) {
      if ($line.Contains('Parameter labels for Extrude hidden')) { Ui-Fail "${Description}: the labels were hidden" }
      if ($line -match '^Parameter label Sketch1 k5 width at (\d+,\d+)$') { $at = $Matches[1] }
    }
    if ($at) { break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 5000)
  if (-not $at) { Ui-Fail "${Description}: the label's place did not change as the camera turned" }
  Write-Host "ok   $Description (at $at)"
  return $at
}
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'names.json'
$design = Join-Path $work 'names.mitcad'
Set-Content -Encoding ascii $script (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 25},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5", "text": [20, -10]},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k6", "text": [30, 12]},
  {"cmd": "sketch.add_dimension", "sketch": "F1", "dimension": {"type": "length", "line": "c4"}, "driven": true, "text": [-10, 12]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 3}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_body"}},
  {"cmd": "set_feature_visible", "uid": "F1", "visible": true}
]
'@ -creplace 'RECT', 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}')
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'names' @('--open', $design)
Ui-ExpectLog 'names.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-ExpectNew 'Palette paletteDimensionText at' 'the palette logs its Dimension text option'
Ui-Mark
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectNew 'Dimension k6 text at' 'the dimensions'' places logged'
Start-Sleep -Milliseconds 800

Write-Host '--- Tooltip, the name in the editor, a dimension picked into it'
Ui-Mark
Ui-Step 'double-click the height'        { Ui-DoubleClickLogged 'Dimension k6 text' }
Ui-ExpectNew 'Dimension tooltip k6: d2 = 25.00 mm (Sketch1 height)' 'the pointer over the height shows its tooltip'
Ui-ExpectNew 'Sketch value editor at' 'the value editor opened'
Ui-ExpectNew 'with d2 = 25' 'the editor shows d2 ='
Ui-ExpectNew 'Sketch value editor tooltip: d2 = 25.00 mm (Sketch1 height)' 'its tooltip says what d2 is'
Ui-Step 'click the width'                { Ui-ClickLogged 'Dimension k5 text' }
Ui-ExpectNew 'Sketch value editor: inserted d1, now d1' 'the width''s name inserted'
Ui-Step 'type /2, Enter'                 { Ui-Type '/2'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew 'Dimension k6 = d1/2 (d2)' 'the height follows the width'

Write-Host '--- Rename in the editor; a taken name refused'
Ui-Mark

Ui-Step 'double-click the width'         { Ui-DoubleClickLogged 'Dimension k5 text' }
Ui-ExpectNew 'with d1 = 40' 'the editor shows d1 ='
Ui-ExpectNew 'Sketch value editor tooltip: d1 = 40.00 mm (Sketch1 width)' 'its tooltip says what d1 is'
Ui-Step 'type width = 50, Enter'         { Ui-Key 'ctrl+a'; Ui-Type 'width = 50'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew 'Dimension k5 = width = 50 mm (width)' 'd1 renamed to width and set'
Ui-Mark
Ui-Step 'double-click the width'         { Ui-DoubleClickLogged 'Dimension k5 text' }
Ui-ExpectNew 'with width = 50' 'the editor shows width ='
Ui-Step 'type d2 = 30, Enter'            { Ui-Key 'ctrl+a'; Ui-Type 'd2 = 30'; Start-Sleep -Milliseconds 200; Ui-Key 'Return' }
Ui-ExpectNew "parameter d2 already exists" 'the taken name refused with a message'
Ui-ExpectNew 'Sketch value editor kept open with d2 = 30' 'the editor stays open'
Start-Sleep -Milliseconds 500
if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Dimension k5 = d2') }) {
  Ui-Fail 'the taken name was accepted'
}
Ui-Step 'Esc closes the editor'          { Ui-Key 'Escape' }

Write-Host '--- Dimension text: Name, then Name = value'
Ui-Mark
Ui-Step 'Dimension text: Name'           { Ui-WheelLogged 'Palette paletteDimensionText' -1 }
Ui-ExpectNew 'Dimension texts: k5 width, k6 d2, k7 (d3)' 'the names shown, the driven one''s reference too'
Ui-Step 'Dimension text: Name = value'   { Ui-WheelLogged 'Palette paletteDimensionText' -1 }
Ui-ExpectNew 'Dimension texts: k5 width = 50.00, k6 d2 = fx: 25.00, k7 (d3 = 25.00)' 'names and values shown'
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'

Write-Host '--- A feature panel''s field with its parameter''s name'
Ui-Mark
Ui-Step 'edit Extrude1 (double-click)'   { Ui-DoubleClickLogged 'Timeline Extrude1' }
Ui-ExpectNew 'Editing F2 with Extrude' 'the extrude opens for editing' 30
Ui-ExpectNew 'Panel Extrude label: Distance (d4)' 'the distance field names d4'
Ui-ExpectNew 'Panel Extrude tooltip distance: d4 = 10 mm (Extrude1 distance)' 'its tooltip says what d4 is'
Ui-Step 'type depth = 12'                { Ui-TypeIn 'Panel Extrude input distance' 'depth = 12' }
Ui-ExpectNew 'Extrude Distance: 12 = 12 mm' 'the value evaluated'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F2' 'the extrude edited' 30
Ui-Mark
Ui-Step 'edit Extrude1 again'            { Ui-DoubleClickLogged 'Timeline Extrude1' }
Ui-ExpectNew 'Panel Extrude label: Distance (depth)' 'd4 renamed to depth' 30
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Write-Host '--- Names picked from the view into a feature panel''s field (mitcad#250, mitcad#587)'
Ui-Mark
Ui-Step 'edit Extrude2 (double-click)'   { Ui-DoubleClickLogged 'Timeline Extrude2' }
Ui-ExpectNew 'Editing F4 with Extrude' 'the second extrude opens for editing' 30
Ui-ExpectNew 'Panel Extrude label: Distance (d6)' 'its distance field names d6'
Ui-ExpectNew 'Panel Extrude tooltip distance: d6 = -4' 'its tooltip says what d6 is'
Ui-ExpectNew '(Extrude2 distance)' 'with its comment'
# The panel gave its first value's field the keyboard as it opened: no
# labels until the field is clicked or typed in (mitcad#587).
if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Parameter labels for Extrude:') }) {
  Ui-Fail 'the view showed parameter labels as the panel opened'
}
Write-Host 'ok   no labels as the panel opened'
Ui-Mark
Ui-Step 'click into the distance'        { Ui-ClickLogged 'Panel Extrude input distance' }
Ui-ExpectNew 'Parameter labels for Extrude: Sketch1 k5 width' 'the click showed the names to pick'
Ui-ExpectNew 'Parameter label Sketch1 k5 width at' 'the shown sketch''s dimensions are labelled'
if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_ -match 'Parameter label (Extrude1|Fillet1) ' }) {
  Ui-Fail 'an earlier feature''s value was labelled (only sketch dimensions, mitcad#587)'
}
Write-Host 'ok   only the sketch''s dimensions are labelled'
# The labels follow the camera as it turns (mitcad#1179): Right View and
# Front View by their keys, the field keeping the keyboard and the labels;
# each time the width's label has a new place, the same for the same view.
$start = Ui-LastMatch '^Parameter label Sketch1 k5 width at (\d+,\d+)$'
Ui-Mark
Ui-Step 'Right View (F7)'                { Ui-Key 'F7' }
Ui-ExpectNew 'Camera direction -1 0 0 up 0 0 1' 'the view turned to the right' 10
$right = Width-LabelFollows 'the width''s label follows to the right view'
if ($right -eq $start) { Ui-Fail "the width's label stayed at $start" }
Ui-Mark
Ui-Step 'Front View (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'Camera direction 0 1 0 up 0 0 1' 'the view turned to the front' 10
$front = Width-LabelFollows 'the width''s label follows to the front view'
if ($front -eq $right) { Ui-Fail "the width's label stayed at $right" }
Ui-Mark
Ui-Step 'Right View again (F7)'          { Ui-Key 'F7' }
Ui-ExpectNew 'Camera direction -1 0 0 up 0 0 1' 'the view turned to the right again' 10
$again = Width-LabelFollows 'the width''s label follows back to the right view'
if ($again -ne $right) { Ui-Fail "the width's label is at $again in the right view, before at $right" }
# Front View again, where the width's label is well inside the view for
# the click on it below.
Ui-Mark
Ui-Step 'Front View again (F8)'          { Ui-Key 'F8' }
Ui-ExpectNew 'Camera direction 0 1 0 up 0 0 1' 'the view turned to the front again' 10
$again = Width-LabelFollows 'the width''s label follows back to the front view'
if ($again -ne $front) { Ui-Fail "the width's label is at $again in the front view, before at $front" }
Write-Host 'ok   the labels follow the camera'
Ui-Step 'type 2* into the distance'      { Ui-TypeIn 'Panel Extrude input distance' '2*' }
Ui-ExpectNew 'Extrude Distance: 2*' 'the typed text evaluated'
Ui-Mark
Ui-Step 'click the width dimension'      { Ui-ClickLogged 'Parameter label Sketch1 k5 width' }
Ui-ExpectNew 'Parameter label picked: width (Sketch1 k5)' 'the width''s label picked'
Ui-ExpectNew 'Panel Extrude input distance: inserted width, now 2*width' 'width inserted into the field'
Ui-ExpectNew 'Extrude Distance: 2*width = 100' 'the distance follows the width'
Ui-Mark
# The features' values are used by their names, typed.
Ui-Step 'type +depth+d5'                 { Ui-Type '+depth+d5' }
Ui-ExpectNew 'Extrude Distance: 2*width+depth+d5 = 115' 'the distance follows all three'
Ui-ExpectNew 'Panel Extrude tooltip distance: d6 = 2*width+depth+d5' 'the tooltip shows the expression'
Ui-ExpectNew '115 mm (Extrude2 distance)' 'with its value and comment'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Ui-Finish 'UI Windows dimension names test'
