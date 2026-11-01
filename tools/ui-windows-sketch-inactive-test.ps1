# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# Input-driven Windows UI test of inactive sketch constraints (mitcad#721)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# as tools/ui-sketch-inactive-test.sh on Linux:
#   1. A design (mitcad-cli): Sketch1 with a fixed point p1 at the origin
#      and a line c2 from it to p3 at (30, 20); a horizontal constraint k1 on
#      c2, made inactive as an import leaves one the file does not meet, and
#      a vertical distance k2 of 20 mm from p1 to p3.
#   2. The sketch opens with k1 drawn inactive.
#   3. Its context menu's Activate is refused (the vertical distance holds
#      the line up); with the vertical distance deleted, Activate applies
#      it; undo makes it inactive again.
#   4. Delete removes it; the saved design has no constraint and no
#      dimension.
#   5. Sketch2 (F2): two horizontal lines and a parallel between them that
#      their horizontals imply. Edited, the Sketch Palette shows the
#      redundant constraint as a warning (mitcad#1051).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-inactive-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-inactive'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'inactive.json'
$design = Join-Path $work 'inactive.mitcad'
# The horizontal constraint made first, then set aside: the line's end is
# dragged back up and dimensioned there.
Set-Content -Encoding ascii $script @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [30, 20]},
  {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "horizontal", "line": "c2"}},
  {"cmd": "sketch.set_active", "sketch": "F1", "items": ["k1"], "active": false,
   "reason": "not met in the imported file"},
  {"cmd": "sketch.drag", "sketch": "F1", "entity": "p3", "to": [30, 20]},
  {"cmd": "sketch.add_dimension", "sketch": "F1", "value": 20,
   "dimension": {"type": "vertical_distance", "a": "p1", "b": "p3"}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [0, -30]},
      {"id": "p2", "type": "point", "at": [30, -30]},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
      {"id": "p4", "type": "point", "at": [0, -40]},
      {"id": "p5", "type": "point", "at": [30, -40]},
      {"id": "c6", "type": "line", "start": "p4", "end": "p5"}],
    "constraints": [
      {"id": "k7", "type": "horizontal", "line": "c3"},
      {"id": "k8", "type": "horizontal", "line": "c6"},
      {"id": "k9", "type": "parallel", "a": "c3", "b": "c6"}]}}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Choose-Entry "logged name" "entry": a right click on what the log placed;
# the context menu stays open and takes the keys (as in
# tools/ui-windows-timber-test.ps1), so the entry is chosen with Down as
# many times as its place in the logged menu, then Enter.
function Choose-Entry([string]$Logged, [string]$Entry) {
  Ui-Mark
  Ui-Step "right-click $Logged"          { Ui-ClickLogged $Logged 3 }
  Ui-ExpectNew 'Context menu: ' 'the context menu opened'
  $entries = @((Ui-LastMatch '^Context menu: (.*)$') -split ' \| ')
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step $Entry {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}

Ui-StartApp 'inactive' @('--open', $design)
Ui-ExpectLog 'inactive.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-ExpectNew 'Sketch inactive drawn: k1 constraint' 'k1 is drawn inactive'
Ui-Mark
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectNew 'Sketch view ' 'the view fitted'
# The fit centres the sketch, so the line's middle, where k1's symbol sits,
# can stay where it was drawn: its place is logged again only when it moves
# (mitcad#726). The last logged place is the one to click.
Ui-ExpectLog 'Constraint k1 glyph at' 'its symbol is placed'
Start-Sleep -Milliseconds 800

Write-Host '--- Activate refused while the vertical distance holds the line up'
Choose-Entry 'Constraint k1 glyph' 'Activate'
Ui-ExpectNew 'Sketch items not activated: k1 cannot be activated' 'refused with the reason'
if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Sketch items activated: k1') }) {
  Ui-Fail 'k1 was activated'
}
Write-Host 'ok   k1 stays inactive'

Write-Host '--- Without the vertical distance, Activate applies it'
Choose-Entry 'Dimension k2 text' 'Delete'
Ui-ExpectNew 'Deleted k2' 'the vertical distance deleted'
Choose-Entry 'Constraint k1 glyph' 'Activate'
Ui-ExpectNew 'Sketch items activated: k1' 'k1 activated'
Ui-ExpectNew 'Sketch inactive drawn: none' 'nothing is drawn inactive'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Activate in Sketch1' 'the activation is one undo step'
Ui-ExpectNew 'Sketch inactive drawn: k1 constraint' 'k1 inactive again'
Ui-ExpectNew 'Constraint k1 glyph at' 'its symbol placed again after the sketch changed (mitcad#723)'

Write-Host '--- Delete removes it'
Choose-Entry 'Constraint k1 glyph' 'Delete'
Ui-ExpectNew 'Deleted k1' 'k1 deleted'
Ui-ExpectNew 'Sketch inactive drawn: none' 'nothing is drawn inactive'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'

$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @'
[
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "constraints": [], "dimensions": []}}}
]
'@
$checked = & $cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved sketch: $($checked -join ' / ')" }
Write-Host 'ok   no constraint and no dimension saved'

Write-Host '--- Redundant constraints: a warning (mitcad#1051)'
Ui-Mark
Ui-Step 'edit Sketch2 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch2' }
Ui-ExpectNew 'Editing sketch F2 (Sketch2)' 'Sketch2 edited' 30
Ui-ExpectNew 'Sketch Palette solve: warning: Redundant constraints (they follow from the others): k' `
  'the Sketch Palette shows the redundant constraint as a warning'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch2 finished'

Ui-Finish 'Windows UI sketch inactive constraint test'
