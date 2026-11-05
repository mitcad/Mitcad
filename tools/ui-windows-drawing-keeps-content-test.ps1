# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/MainWindow.cpp core/model/src/drawing core/model/src/document/drawings.rs
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing.rs core/model-api/src/lib.rs tools/cli
# Input-driven Windows UI test of a drawing that keeps what was put into it
# (mitcad#1293, GitHub #144) in the build VM (tools/ui-windows-lib.ps1 says
# how the input is posted), as tools/ui-drawing-keeps-content-test.sh does
# on Linux, on Plate (a 40 x 30 x 5 body, F2.b0) placed twice, Plate:1 (O1)
# and Plate:2 (O2) 50 mm apart, that mitcad-cli makes:
#   1. Create Drawing (a shortcut of the test's) and a base view of the
#      design: it draws both plates (its projection logged with 2 bodies).
#   2. Close Drawing, Plate:2 hidden in the browser (its light bulb), the
#      drawing opened again: the view still draws both.
#   3. Close Drawing, Plate:1 copied and pasted in the browser, 100 mm along
#      X (Plate:3, O3), the drawing opened again: the view still draws two.
#   4. The view's Shows: Plate:3 checked (its row, Space): three bodies, one
#      undo step; Ctrl+Z takes it away.
#   5. Plate:2 shown again; Update from Model: the three shown, one undo
#      step (Update view 1 of Drawing1); Ctrl+Z takes Plate:3 away again.
#   6. A design file written before views recorded their bodies (mitcad-cli
#      makes it with Plate:2 hidden, its view's subject taken out): opened,
#      the view is pinned to what the file shows (one body) and the app says
#      so; Plate:2 shown, the view still draws one.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-keeps-content-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-keeps-content'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'file.create_drawing' = 'F9' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

$plates = (@'
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "copy_occurrence", "occurrence": "Plate:1", "transform": {"translation": [50, 0, 0]}}
'@ -creplace 'RECT', $region)
$modelScript = Join-Path $work 'plates.json'
$design = Join-Path $work 'plates.mitcad'
Set-Content -Encoding ascii $modelScript "[`n$plates`n]"
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# The older file of step 6: Plate:2 hidden, a drawing with a front view,
# the view's subject taken out of the file as mitcad-cli saved it.
$olderScript = Join-Path $work 'older.json'
$recorded = Join-Path $work 'recorded.mitcad'
$older = Join-Path $work 'older.mitcad'
Set-Content -Encoding ascii $olderScript @"
[
$plates,
  {"cmd": "set_occurrence_visible", "occurrence": "Plate:2", "visible": false},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front", "at": [150, 150], "scale": "1:1"}
]
"@
$made = & $cli run $olderScript --save $recorded
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $recorded)) { Ui-Fail "mitcad-cli (the older file): $($made -join ' / ')" }
$text = [IO.File]::ReadAllText($recorded)
$subject = [regex]::new(',\s*"subject":\s*\{\s*"type":\s*"bodies",\s*"bodies":\s*\[[^\]]*\]\s*\}')
$oldText = $subject.Replace($text, '', 1)
if ($oldText -eq $text) { Ui-Fail 'the saved view has no subject of its bodies to take out' }
[IO.File]::WriteAllText($older, $oldText)
Write-Host 'ok   an older design file made: its view without a subject'

# Sheet-At x y: the window point of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)), as tools/ui-windows-drawings-test.ps1 has it.
function Sheet-At([double]$X, [double]$Y) {
  $view = Ui-LastMatch 'Drawing sheet view ([-0-9.,]+ [-0-9.,]+ [-0-9.,]+)'
  if (-not $view) { Ui-Fail 'the app did not log the drawing''s sheet' }
  $p = $view -split ' ' | ForEach-Object { , ($_ -split ',' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) }) }
  $px = $p[0][0] + ($p[1][0] - $p[0][0]) * $X / 100 + ($p[2][0] - $p[0][0]) * $Y / 100
  $py = $p[0][1] + ($p[1][1] - $p[0][1]) * $X / 100 + ($p[2][1] - $p[0][1]) * $Y / 100
  return @([int][Math]::Round($px), [int][Math]::Round($py))
}

function Sheet-Click([double]$X, [double]$Y) {
  $at = Sheet-At $X $Y
  Ui-Click $at[0] $at[1]
  Ui-Sync
}

# Settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
function Settled([string]$Text) {
  for ($i = 0; $i -lt 20; $i++) {
    $before = Ui-LastLineWith "$Text at "
    Ui-Sync
    Start-Sleep -Milliseconds 300
    $after = Ui-LastLineWith "$Text at "
    if ($after -and $before -eq $after) { return }
  }
  Ui-Fail "the place of '$Text' did not settle"
}

# View-Draws view bodies: since the mark, the log's line of the view's
# projection (mitcad#1033) with that many bodies, the kernel's or the
# cache's.
function View-Draws([int]$View, [int]$Bodies, [string]$Why) {
  $pattern = "^Drawing projection of view ${View}: $Bodies bodies, [0-9]+\.[0-9] ms, (projected exactly|projected from a triangulation|from the cache)$"
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    foreach ($line in @(Ui-NewLines)) {
      if ($line -cmatch $pattern) { Write-Host "ok   view $View draws $Bodies bodies: $Why"; return }
    }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Ui-Fail "view $View does not draw $Bodies bodies ($Why): no such line of its projection in the log"
}

# Shown-Bodies "list": the 3D view's last "Bodies shown:" line, waited for,
# is that list.
function Shown-Bodies([string]$List) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $last = Ui-LastLineWith 'Bodies shown: ' -Start
    if ("$last" -eq "Bodies shown: $List") { Write-Host "ok   the 3D view shows $List"; return }
    Start-Sleep -Milliseconds 100
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Ui-Fail "the 3D view shows '$last', not '$List'"
}

# Menu-Choose "entry": the context menu's entry chosen with the keys, as
# the menu opens with nothing active (tools/ui-windows-move-copy-menu-test.ps1).
$ellipsis = [string][char]0x2026
function Menu-Choose([string]$Entry) {
  $entries = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ' | ForEach-Object { $_.Replace($ellipsis, '...') })
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step $Entry {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}

function Close-Drawing {
  Ui-Mark
  Settled 'Drawing Close Drawing'
  Ui-Step 'Close Drawing'                 { Ui-ClickLogged 'Drawing Close Drawing' }
  Ui-ExpectNew 'Drawing closed' 'the 3D view again'
}

function Open-Drawing {
  Ui-Mark
  Ui-Step 'open Drawing1 (double-click)'  { Ui-DoubleClickLogged 'Browser Root/Drawings/Drawing1' }
  Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'the drawing opened'
  # The sheet's place is logged only when it changes: not again here.
  Ui-ExpectNew 'Drawing sheet drawn: ' 'its sheet drawn'
}

function No-Refusal {
  if (Ui-NewLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }
}

Ui-StartApp 'drawing-keeps-content' @('--open', $design)
Ui-ExpectLog 'plates.mitcad' 'the design opened'
Shown-Bodies 'F2.b0 in O1, F2.b0 in O2'

Write-Host '--- 1. A base view of the design'
Ui-Mark
Ui-Step 'Create Drawing (F9)'             { Ui-Key 'F9' }
Ui-ExpectNew 'Create Drawing dialog opened' 'its dialog asks for the sheet'
Ui-Step 'OK'                              { Ui-FocusDialog '^Create Drawing$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'its sheet over the view'
Ui-ExpectNew 'Drawing sheet view ' 'the sheet placed'
Ui-ExpectNew 'Drawing tool: Base View' 'Base View on for the new drawing'
Ui-Mark
Ui-Step 'click the sheet at 150, 150'     { Sheet-Click 150 150 }
Ui-ExpectNew '"type":"base"' 'the base view asked for'
Ui-ExpectNew 'Drawing view 1 at' 'the front view drawn'
View-Draws 1 2 'both plates'
Ui-ExpectNew 'Drawing tool: Projected View' 'projected views follow'
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Ui-ExpectNew 'Drawing tool: selection' 'back to the selection'
No-Refusal

Write-Host '--- 2. Plate:2 hidden in the browser'
Close-Drawing
Ui-Mark
Ui-Step 'hide Plate:2 (light bulb)'       { Ui-ClickLogged 'Browser eye Root/Plate:2' }
Shown-Bodies 'F2.b0 in O1'
Open-Drawing
View-Draws 1 2 'hidden in the 3D view, Plate:2 still drawn'

Write-Host '--- 3. A placement added: Plate:1 copied and pasted'
Close-Drawing
Ui-Mark
Ui-Step 'right-click Plate:1'             { Ui-ClickLogged 'Browser Root/Plate:1' 3 }
Ui-ExpectNew 'Context menu: ' 'its menu'
Menu-Choose 'Copy'
Ui-ExpectNew 'Copied Plate:1' 'copied'
Ui-Mark
Ui-Step 'right-click Root'                { Ui-ClickLogged 'Browser Root' 3 }
Ui-ExpectNew 'Context menu: ' 'its menu'
Menu-Choose 'Paste'
Ui-ExpectNew 'Pasted Plate:1 as Plate:3' 'a third placement'
Ui-ExpectNew 'Panel Move/Copy input tx at' 'Move/Copy opens on it'
Ui-Step '100 mm along X'                  { Ui-TypeIn 'Panel Move/Copy input tx' '100' }
Ui-ExpectNew 'Preview Move/Copy: ok' 'previewed'
Ui-Step 'OK (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Placed Plate:3 at (100, 0, 0)' 'placed 100 mm along X'
Shown-Bodies 'F2.b0 in O1, F2.b0 in O3'
Open-Drawing
View-Draws 1 2 'the new placement not drawn'

Write-Host '--- 4. Shows adds it'
Ui-Mark
Ui-Step 'select the front view'           { Ui-ClickLogged 'Drawing view 1' }
Ui-ExpectNew 'Drawing view 1 selected' 'the front view selected'
Ui-ExpectNew 'Drawing view 1 shows 2 of 3 bodies' 'its Shows: Plate:1 and Plate:2 of the three'
Ui-ExpectNew 'Drawing panel drawingViewShows row Plate:3 at' "Plate:3's row in its Shows"
Ui-ExpectNew 'Drawing panel drawingViewUpdate at' 'Update from Model beside it'
Settled 'Drawing panel drawingViewShows row Plate:3'
Ui-Mark
Ui-Step "Plate:3's row"                   { Ui-ClickLogged 'Drawing panel drawingViewShows row Plate:3' }
Ui-Step 'check it (Space)'                { Ui-Key 'space' }
Ui-ExpectNew 'Drawing view 1 Shows changed: 3 bodies' 'the three plates sent'
Ui-ExpectNew '"cmd":"edit_drawing_view"' 'one edit of the view'
View-Draws 1 3 'Plate:3 added by its Shows'
No-Refusal
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                   { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Edit view 1 of Drawing1' 'one undo step'
View-Draws 1 2 'Plate:3 away again'

Write-Host '--- 5. Update from Model'
Close-Drawing
Ui-Mark
Ui-Step 'show Plate:2 (light bulb)'       { Ui-ClickLogged 'Browser eye Root/Plate:2' }
Shown-Bodies 'F2.b0 in O1, F2.b0 in O2, F2.b0 in O3'
Open-Drawing
View-Draws 1 2 'shown again, nothing added'
Ui-Mark
Ui-Step 'select the front view'           { Ui-ClickLogged 'Drawing view 1' }
Ui-ExpectNew 'Drawing view 1 selected' 'the front view selected'
Ui-ExpectNew 'Drawing panel drawingViewUpdate at' 'Update from Model in its panel'
Settled 'Drawing panel drawingViewUpdate'
Ui-Mark
Ui-Step 'Update from Model'               { Ui-ClickLogged 'Drawing panel drawingViewUpdate' }
Ui-ExpectNew 'Drawing view 1 updated from the model' 'the button pressed'
Ui-ExpectNew '"subject":{"type":"design"}' 'what the 3D view shows asked for'
View-Draws 1 3 'the three plates shown in the 3D view'
Ui-ExpectNew 'Drawing view 1 shows 3 of 3 bodies' 'its Shows has all three'
No-Refusal
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                   { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Update view 1 of Drawing1' 'one undo step'
View-Draws 1 2 'Plate:3 away again'

Write-Host '--- 6. A design file written before views kept their bodies'
Ui-StartApp 'drawing-keeps-content-older' @('--open', $older)
Ui-ExpectLog 'older.mitcad' 'the older design opened'
Ui-ExpectLog 'Drawing pinned: Drawing1 now keeps what its views showed when the design was opened (1 body)' 'the view pinned, and said so'
Shown-Bodies 'F2.b0 in O1'
Ui-Mark
Ui-Step 'show Plate:2 (light bulb)'       { Ui-ClickLogged 'Browser eye Root/Plate:2' }
Shown-Bodies 'F2.b0 in O1, F2.b0 in O2'
Open-Drawing
View-Draws 1 1 'what the file showed: Plate:1 alone'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows drawing keeps content test'
