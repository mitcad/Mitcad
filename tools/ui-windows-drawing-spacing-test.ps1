# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-drawing/src/spacing.rs core/model-drawing/src/annotate.rs
# check-all sources: core/model-api/src/drawing_spacing.rs core/model-api/src/drawing_annotations.rs tools/cli
# Input-driven Windows UI test of dimension spacing (mitcad#1173) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), the
# steps of tools/ui-drawing-dimension-spacing-test.sh, on a 40 x 30 x 20
# block mitcad-cli makes; its front view at 2:1 has its corners at
# (80, 100), (160, 100), (160, 140) and (80, 140) on the sheet, so a
# dimension between two corners of a side has its line_offset as far out
# as it lies beyond the view's box:
#   1. The first dimension (the top corners) 12 mm above the view: where
#      it is clicked.
#   2. The second (the bottom corners) clicked 13.5 mm below: its preview
#      snaps and it is placed at the first's spacing, its line_offset as
#      large as the first's.
#   3. The third (the right side) two spacings and 1 mm out: two spacings.
#   4. The fourth (the left side) Alt+clicked 2.5 mm off the spacing:
#      where it is clicked.
#   5. The fourth dragged near two spacings out: it snaps to the third's.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-spacing-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-spacing'),
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

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'block.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

$invariant = [Globalization.CultureInfo]::InvariantCulture

# Sheet-At x y: the window point of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)).
function Sheet-At([double]$X, [double]$Y) {
  $view = Ui-LastMatch 'Drawing sheet view ([-0-9.,]+ [-0-9.,]+ [-0-9.,]+)'
  if (-not $view) { Ui-Fail 'the app did not log the drawing''s sheet' }
  $p = $view -split ' ' | ForEach-Object { , ($_ -split ',' | ForEach-Object { [double]::Parse($_, $invariant) }) }
  $px = $p[0][0] + ($p[1][0] - $p[0][0]) * $X / 100 + ($p[2][0] - $p[0][0]) * $Y / 100
  $py = $p[0][1] + ($p[1][1] - $p[0][1]) * $X / 100 + ($p[2][1] - $p[0][1]) * $Y / 100
  return @([int][Math]::Round($px), [int][Math]::Round($py))
}

function Sheet-Click([double]$X, [double]$Y) {
  $at = Sheet-At $X $Y
  Ui-Click $at[0] $at[1]
  Ui-Sync
}

# A move with nothing in wParam is dropped by Qt in session 0 (no window
# under the cursor): MK_SHIFT keeps it a plain move, as in
# tools/ui-windows-joint-highlight-test.ps1 (Qt reads the modifiers from
# the key state).
function Sheet-Hover([double]$X, [double]$Y) {
  $at = Sheet-At $X $Y
  [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0x04)
  Ui-Sync
}

# Sheet-AltClick x y: a click with Alt held.
function Sheet-AltClick([double]$X, [double]$Y) {
  $at = Sheet-At $X $Y
  Ui-ClickWith 'alt' $at[0] $at[1]
  Ui-Sync
}

# Sheet-DragBy "logged place" dx dy: a left drag from a logged place (an
# annotation's) by that many sheet millimetres.
function Sheet-DragBy([string]$Text, [double]$Dx, [double]$Dy) {
  $from = Ui-LoggedAt $Text
  $origin = Sheet-At 0 0
  $by = Sheet-At $Dx $Dy
  Ui-Drag $from[0] $from[1] ($from[0] + $by[0] - $origin[0]) ($from[1] + $by[1] - $origin[1])
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

# Drawing-Tool "name" "group": the tool from the ribbon's DRAWING tab, by
# its button when the tab shows it, else by its row in its group's menu
# (session 0's 1024x768 desktop collapses the right groups to their names),
# as tools/ui-windows-drawings-test.ps1 does.
function Drawing-Tool([string]$Name, [string]$Group) {
  Settled "Drawing group $Group"
  $lines = @(Ui-LogLines)
  $start = -1
  for ($i = $lines.Count - 1; $i -ge 0; $i--) {
    if ($lines[$i].Contains('Drawing area ')) { $start = $i; break }
  }
  $shown = $start -ge 0 -and @($lines | Select-Object -Skip $start | Where-Object { $_.Contains("Drawing tool $Name at ") }).Count -gt 0
  if ($shown) {
    $button = "Drawing tool $Name"
    Settled $button
    Ui-Step $Name                           { Ui-ClickLogged $button }
    return
  }
  $path = 'DRAWING/' + $Group.ToUpperInvariant()
  Ui-Step "the $Group menu (the group shows its name only)" { Ui-ClickLogged "Drawing group $Group" }
  Ui-ExpectNew "Ribbon menu ${path}: " "the $Group group's menu"
  $menu = "$(Ui-LastLineWith "Ribbon menu ${path}: ")"
  $entries = $menu.Substring($menu.IndexOf("Ribbon menu ${path}: ") + "Ribbon menu ${path}: ".Length)
  $rows = @($entries -split ' \| ' | Where-Object { $_ -ne '-' })
  $row = -1
  for ($i = 0; $i -lt $rows.Count; $i++) {
    if ($rows[$i] -eq $Name -or $rows[$i].StartsWith("$Name (")) { $row = $i; break }
  }
  if ($row -lt 0) { Ui-Fail "no $Name in the $Group group's menu: $entries" }
  Ui-Step "$Name (Down x$($row + 1), Enter)" { Ui-Key (@('Down') * ($row + 1)); Ui-Key 'Return' }
}

# Offset-Of verb: the line_offset of the last "Drawing dimension <verb>:"
# line, without its sign.
function Offset-Of([string]$Verb) {
  $text = Ui-LastMatch "Drawing dimension ${Verb}: line_offset (-?[0-9.]+)"
  if (-not $text) { Ui-Fail "no 'Drawing dimension ${Verb}' line in the log" }
  return [Math]::Abs([double]::Parse($text, $invariant))
}

# Same a b what: two distances (mm) logged to 0.01 the same, or fail (their
# last digits may differ by the rounding of each).
function Same([double]$A, [double]$B, [string]$What) {
  if ([Math]::Abs($A - $B) -gt 0.025) { Ui-Fail "${What}: $A is not $B" }
  Write-Host "ok   ${What}: $A"
}

function No-Refusal {
  if (Ui-NewLines | Where-Object { $_.Contains('Drawing command refused') }) { Ui-Fail 'a drawing command was refused' }
}

# Dimension-Between x1 y1 x2 y2: the two corners picked with Dimension.
function Dimension-Between([double]$X1, [double]$Y1, [double]$X2, [double]$Y2) {
  Ui-Step "a corner at $X1, $Y1"            { Sheet-Click $X1 $Y1 }
  Ui-ExpectNew 'Drawing picked point (1)' 'the first corner picked'
  Ui-Step "a corner at $X2, $Y2"            { Sheet-Click $X2 $Y2 }
  Ui-ExpectNew 'Drawing picked point (2)' 'the second corner picked'
  Ui-ExpectNew 'Drawing placing view' 'its place is clicked next'
}

Ui-StartApp 'drawing-spacing' @('--open', $design)
Ui-ExpectLog 'block.mitcad' 'the design opened'

Write-Host '--- A drawing with the front view at 2:1'
Ui-Mark
Ui-Step 'Create Drawing (F9)'             { Ui-Key 'F9' }
Ui-ExpectNew 'Create Drawing dialog opened' 'its dialog asks for the sheet'
Ui-Step 'OK: A3, landscape, first angle'  { Ui-FocusDialog '^Create Drawing$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Drawing opened: D1 Drawing1, sheet 1 of 1' 'its sheet over the view'
Ui-ExpectNew 'Drawing sheet view ' 'the sheet placed'
Ui-ExpectNew 'Drawing tool: Base View' 'Base View is on'
Ui-Mark
Ui-Step 'click the sheet at 120, 120'     { Sheet-Click 120 120 }
Ui-ExpectNew 'Drawing view 1 at' 'the front view drawn'
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Ui-ExpectNew 'Drawing tool: selection' 'back to the selection'

Write-Host '--- 1. The first dimension: where it is clicked'
Ui-Mark
Drawing-Tool 'Dimension' 'Dimensions'
Ui-ExpectNew 'Drawing tool: Dimension' 'the dimension tool is on'
Dimension-Between 80 140 160 140
Ui-Step 'point 12 mm above the view'      { Sheet-Hover 120 152 }
Ui-ExpectNew 'Drawing preview: dimension' 'its lines follow the pointer'
Ui-Step 'where it goes'                   { Sheet-Click 120 152 }
Ui-ExpectNew 'Drawing dimension placed: line_offset' 'its place logged'
Ui-ExpectNew ', not snapped' 'nothing to snap to: the first dimension'
Ui-ExpectNew 'Drawing annotations drawn: 1 = 40' 'drawn: the block is 40 wide'
No-Refusal
$first = Offset-Of 'placed'
if ($first -le 10.5 -or $first -ge 13.5) { Ui-Fail "the first dimension is not about 12 mm out: $first" }
Write-Host "ok   the first dimension $first mm out"

Write-Host '--- 2. The second: at the first''s spacing'
Ui-Mark
Dimension-Between 80 100 160 100
Ui-Step 'point 13.5 mm below the view'    { Sheet-Hover 120 86.5 }
Ui-ExpectNew 'Drawing snap: ' 'the preview snapped'
Ui-ExpectNew ' mm x1' 'one spacing out'
Ui-Step 'where it goes'                   { Sheet-Click 120 86.5 }
Ui-ExpectNew 'x1 of annotation 1' 'snapped to the first''s spacing'
Ui-ExpectNew 'Drawing annotations drawn: 1 = 40 | 2 = 40' 'drawn'
No-Refusal
Same (Offset-Of 'placed') $first 'the second''s line_offset is the first''s'

Write-Host '--- 3. The third: two spacings out'
Ui-Mark
Dimension-Between 160 140 160 100
Ui-Step 'click two spacings and 1 mm out' { Sheet-Click (160 + 2 * $first + 1) 120 }
Ui-ExpectNew 'x2 of annotation 2' 'snapped to two spacings'
No-Refusal
Same (Offset-Of 'placed') ([Math]::Round(2 * $first, 2)) 'the third''s line_offset is twice the first''s'

Write-Host '--- 4. The fourth with Alt: where it is clicked'
Ui-Mark
Dimension-Between 80 140 80 100
Ui-Step 'Alt+click 2.5 mm beyond a spacing' { Sheet-AltClick (80 - $first - 2.5) 120 }
Ui-ExpectNew ', free' 'placed freely'
Ui-ExpectNew 'Drawing annotations drawn: 1 = 40 | 2 = 40 | 3 = 20 | 4 = 20' 'drawn'
No-Refusal
$fourth = Offset-Of 'placed'
if ([Math]::Abs($fourth - $first) -le 1) { Ui-Fail "the fourth snapped with Alt: $fourth" }
Write-Host "ok   the fourth $fourth mm out, not at the spacing"

Write-Host '--- 5. The fourth dragged: it snaps'
Ui-Mark
Ui-Step 'end the tool (Esc)'              { Ui-Key 'Escape' }
Ui-ExpectNew 'Drawing tool: selection' 'back to the selection'
Ui-Mark
Settled 'Drawing annotation 4'
Ui-Step 'drag it near two spacings out'   { Sheet-DragBy 'Drawing annotation 4' (-($first - 2.0)) 0 }
Ui-ExpectNew 'Drawing annotation 4 dragged by' 'the dimension dragged'
Ui-ExpectNew 'x1 of annotation 3' 'snapped to the third''s spacing'
No-Refusal
Same (Offset-Of 'moved') ([Math]::Round(2 * $first, 2)) 'the dragged one''s line_offset is twice the first''s'

Ui-Finish 'UI Windows drawing dimension spacing'
