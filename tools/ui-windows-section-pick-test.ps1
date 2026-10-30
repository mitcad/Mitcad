# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/view geometry/analysis tools/cli
# check-all sources: core/cpp/bridge/analysis.cpp core/cpp/bridge/analysis.hpp
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp
# check-all sources: app/MainWindowSection.cpp app/framework/SectionWorker.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/SectionPreview.cpp app/framework/SectionPreview.hpp
# Input-driven Windows UI test of picks on clipped bodies (mitcad#233), on
# a design mitcad-cli makes: Body1 (F2.b0) 30 x 20 x 10 on XY and Body2
# (F5.b0) 10 x 10 x 10 on a plane 5 above XY, beside Body1, and an empty
# Sketch3 on Body1's top face.
#   1. Sketch3 edited with Hide Above Sketch on: Body2 is cut
#      at the sketch plane (its cap), Body1's top face lies on it and stays
#      whole. Project picks that face (well inside it) and a bottom edge of
#      Body2, which the cut kept, both by the bodies' own names, and the
#      sketch gets both projections.
#   2. Section Analysis at x = 15: Fillet picks the top face of what is
#      left of Body1 by its own name. A feature preview at the section
#      (mitcad#943): Chamfer on the cut Body1's top front edge; the
#      preview's body is clipped at the plane on the section worker's
#      thread and the caps follow it (area 199.5 instead of 200); Escape
#      brings back the caps at the marker.
#   3. The clips off the UI thread (mitcad#438), each view round made to
#      wait 4 s: a new distance typed while the round at x = 0 runs stops
#      it; the bodies are clipped and meshed at x = 15 on the section
#      worker's thread, shown, then measured; Cancel while the round at
#      x = 12 runs shows and measures nothing of it.
# Mitcad runs with Mesa's software OpenGL (MITCAD_MESA_DIR) in session 0
# over SSH, as tools/ui-windows-workflow-test.ps1 does.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-section-pick-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-section-pick'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'view.iso' = 'Shift+F3'; 'inspect.section' = 'Shift+F4'; 'solid.chamfer' = 'Shift+F5' } -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$workLogged = $work -replace '\\', '/'

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$modelScript = Join-Path $work 'blocks.json'
$design = Join-Path $work 'blocks.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 5}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [40, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"body": "F2.b0",
    "face": "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Pick-PlaceOf "start" "part": where a click picks the item whose "Pick ..."
# line (after the last "Pick places:") starts with the text and has the
# part too (Ui-PickAt matches one text only).
function Pick-PlaceOf([string]$Start, [string]$Part) {
  $lines = Ui-LogLines
  $first = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $first = $i; break } }
  $line = $lines[$first..($lines.Count - 1)] |
    Where-Object { $_.Contains("Pick $Start") -and $_.Contains($Part) } | Select-Object -Last 1
  # "$line": with no line, -notmatch on the empty result is not true
  # (Ui-PickAt, mitcad#525).
  if ("$line" -notmatch ' at (\d+),(\d+)$') { Ui-Fail "no pick place logged for '$Start' with '$Part'" }
  return @([int]$Matches[1], [int]$Matches[2])
}

Ui-StartApp 'section-pick' @('--open', $design)
Ui-ExpectLog "Opened $workLogged/blocks.mitcad" 'the design opened'
Ui-Step 'isometric view (Shift+F3)'         { Ui-Key 'shift+F3' }
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0, F5.b0' 'both bodies shown'

Write-Host "--- Hide Above Sketch on Body1's top face"
Ui-Mark
Ui-Step 'edit Sketch3 (double-click)'       { Ui-DoubleClickLogged 'Timeline Sketch3' }
Ui-ExpectNew 'Editing sketch F6 (Sketch3)' "the sketch on Body1's top face"
Ui-ExpectNew 'Palette paletteHideAbove at' 'the option is in the palette'
Ui-Mark
Ui-Step 'check Hide Above Sketch'           { Ui-ClickLogged 'Palette paletteHideAbove' }
Ui-ExpectNew 'Hide Above Sketch on' 'on'
Ui-ExpectNew 'Section caps: 1 face(s)' "Body2's cut face as a cap, not Body1's top face"
# The sketch looks at its plane: back to the isometric view, so that
# Body2's sides show.
Ui-Step 'isometric view (Shift+F3)'         { Ui-Key 'shift+F3' }
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'project (P)'                       { Ui-Key 'p' }
Ui-ExpectNew 'Pick places:' 'Project logs where its picks are'
Ui-ExpectNew 'Pick face F2.b0/F2:end(' "Body1's top face can be picked by its name"
Ui-Step "pick Body1's top face"             { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Project Geometry: 1 face' 'the face went to the projection'
Ui-Step "pick a bottom edge of the cut Body2" {
  $at = Pick-PlaceOf 'edge F5.b0/E{' '|F5:start('
  Ui-Click $at[0] $at[1]
}
Ui-ExpectNew 'Project Geometry: 1 face, 1 edge' 'the edge went to the projection'
Ui-ExpectNew 'Preview Project: ok' 'projection previewed'
Ui-Step 'OK (Enter)'                        { Ui-Key 'Return' }
Ui-ExpectNew 'Projected 2 item(s)' 'the sketch got both projections'
Ui-Step 'finish sketch (Ctrl+Enter)'        { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-ExpectNew 'Section caps: none' 'the bodies whole again'

Write-Host '--- Section Analysis'
Ui-Step 'isometric view (Shift+F3)'         { Ui-Key 'shift+F3' }
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'section analysis (Shift+F4)'       { Ui-Key 'shift+F4' }
Ui-ExpectNew 'Datum yz at' 'the origin planes shown'
Ui-Step 'pick YZ'                           { Ui-ClickLogged 'Datum yz' }
Ui-ExpectNew 'Section analysis at' 'the bodies shown cut'
Ui-Mark
Ui-Step '15 along X'                        { Ui-TypeIn 'Panel Section Analysis input offset' '15' }
Ui-ExpectNew 'Section caps: 1 face(s)' "Body1's cut face as a cap, Body2 cut away"
Ui-Step 'OK (Enter)'                        { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Section Analysis' 'closed, the section kept'
Ui-Mark
Ui-Step 'fillet (F)'                        { Ui-Key 'f' }
Ui-ExpectNew 'Pick places:' 'Fillet logs where its picks are'
Ui-ExpectNew 'Pick face F2.b0/F2:end(' "the cut Body1's top face can be picked by its name"
Ui-Step "pick the cut Body1's top face"     { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Fillet Edges: 1 face' 'the face picked by its own name'
Ui-Step 'cancel (Esc)'                      { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Fillet cancelled' 'Fillet cancelled'
Ui-WaitIdle

Write-Host '--- A feature preview at the section (mitcad#943)'
# Chamfer on the top front edge of the cut Body1: the preview's body is
# clipped at x = 15 on the section worker's thread, and its caps follow it:
# 199.5 mm² with the 1 mm bevel across the cut, 200 without.
Ui-Mark
Ui-Step 'chamfer (Shift+F5)'                { Ui-Key 'shift+F5' }
Ui-ExpectNew 'Pick places:' 'Chamfer logs where its picks are'
Ui-Step "pick the cut Body1's top front edge" {
  $at = Pick-PlaceOf 'edge F2.b0/E{F2:end(' 'F2:side(c1[c4,c2])'
  Ui-Click $at[0] $at[1]
}
Ui-ExpectNew 'Chamfer Edges: 1 edge' "the cut Body1's edge picked"
Ui-ExpectNew 'Preview Chamfer: ok' 'previewed'
Ui-ExpectNew 'Section preview ready at ' "the preview's body clipped and handed to the view" 12
Ui-ExpectNew ', area 199.5 (preview)' 'the caps follow the preview'
$since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
if (-not ($since | Where-Object {
      $_ -match '^Section preview clips at .* off the UI thread \(section worker\): 1 bodies, 1 new' })) {
  Ui-Fail "the chamfered body was not clipped on the section worker's thread"
}
if (-not ($since | Where-Object { $_.StartsWith('Section caps: 1 face(s) at') -and $_.EndsWith('(preview)') })) {
  Ui-Fail "the preview's caps are not Body1's cut"
}
if ($since | Where-Object { $_.StartsWith('Section: F2.b0 shown whole') }) {
  Ui-Fail "the preview's clip failed"
}
Write-Host "ok   the chamfered Body1 drawn cut, its caps the preview's"
Ui-Mark
Ui-Step 'cancel (Esc)'                      { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Chamfer cancelled' 'the chamfer cancelled'
Ui-ExpectNew ', area 200.0' "the caps of Body1 at the marker again"

# Line-Of "text": the index, among the lines logged since the mark, of the
# first one that has the text; -1 when none has.
function Line-Of([string]$Text) {
  $lines = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
  for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i].Contains($Text)) { return $i } }
  return -1
}

# The clips off the UI thread (mitcad#438), each view round made to wait
# 4 s before it clips (MITCAD_TEST_SECTION_DELAY_MS), so that the panel
# changes and cancels while one runs; those steps send their keys without
# waiting for the app to be idle.
Write-Host '--- Section Analysis off the UI thread: a newer distance supersedes the round that runs'
$env:MITCAD_TEST_SECTION_DELAY_MS = '4000'
Ui-StartApp 'section-rounds' @('--open', $design)
Remove-Item Env:MITCAD_TEST_SECTION_DELAY_MS
Ui-ExpectLog "Opened $workLogged/blocks.mitcad" 'the design opened again'
Ui-Step 'isometric view (Shift+F3)'         { Ui-Key 'shift+F3' }
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'section analysis (Shift+F4)'       { Ui-Key 'shift+F4' }
Ui-ExpectNew 'Datum yz at' 'the origin planes shown'
Ui-Step 'pick YZ'                           { Ui-ClickLogged 'Datum yz' }
Ui-ExpectNew 'Computing section view started' 'the clips at x = 0 run'
Ui-TypeIn 'Panel Section Analysis input offset' '15'
Ui-Sync
Ui-ExpectNew 'Section round view superseded' 'the round at x = 0 told to stop'
Ui-ExpectNew 'Computing section view cancelled after' 'and ended as cancelled'
$at15 = '{"plane":{"normal":[1,0,0],"origin":[15,0,0]}}'
# Body1 cut, Body2 wholly in front (nothing left of it): both new shapes.
Ui-ExpectNew "Section clips at $at15 off the UI thread (section worker): 2 bodies, 2 new, 2 meshed in" `
  "the bodies clipped and meshed at x = 15 on the section worker's thread" 12
Ui-ExpectNew "Section view ready at ${at15}: 2 bodies from the section worker" 'handed to the view'
Ui-ExpectNew 'Section caps: 1 face(s)' "Body1's cut face as a cap, Body2 cut away"
Ui-ExpectNew 'Inspect Section Analysis: area 200, length 60' "then the panel's numbers" 12
Ui-ExpectNew 'Section measured off the UI thread (section worker)' "measured on the worker's thread"
$since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
if ($since | Where-Object { $_.Contains('"origin":[0,0,0]}} off the UI thread') }) {
  Ui-Fail "the superseded round's clips were made"
}
if (@($since | Where-Object { $_.StartsWith('Inspect Section Analysis: area ') }).Count -ne 1) {
  Ui-Fail 'the panel was measured more than once'
}
$ready = Line-Of "Section view ready at $at15"
$caps = Line-Of 'Section caps: 1 face(s)'
$measured = Line-Of 'Inspect Section Analysis: area 200, length 60'
if ($ready -lt 0 -or $ready -gt $caps -or $caps -gt $measured) {
  Ui-Fail "not shown and then measured in this order: ready $ready, caps $caps, measured $measured"
}
Write-Host "ok   nothing of the superseded round; the numbers came after the view's update"

Write-Host '--- Cancel while a round runs'
Ui-WaitIdle
Ui-Mark
Ui-TypeIn 'Panel Section Analysis input offset' '12'
Ui-Sync
Ui-ExpectNew 'Computing section view started' 'the clips at x = 12 run'
Ui-Key 'Escape'
Ui-ExpectNew 'Command Section Analysis cancelled' 'the panel cancelled while they run'
Ui-ExpectNew 'Computing section view cancelled after' 'the round ended as cancelled'
Ui-ExpectNew 'Section caps: none' 'the bodies whole again'
Ui-WaitIdle
Ui-Sync
if (Ui-Crashed) { Ui-Fail 'crashed after the cancel' }
$since = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
if ($since | Where-Object { $_.Contains('"origin":[12,0,0]}}: 2 bodies from the section worker') }) {
  Ui-Fail "the cancelled round's clips were shown"
}
if ($since | Where-Object { $_.StartsWith('Inspect Section Analysis: area ') }) {
  Ui-Fail "the cancelled round's section was measured"
}
Write-Host 'ok   nothing of the cancelled round shown, the app goes on'

Ui-Finish 'UI Windows section pick test'
