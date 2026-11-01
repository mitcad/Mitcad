# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: app/MainWindow.cpp
# Input-driven Windows UI test of the constraints the sketch tools add
# because the drawn geometry already satisfies them, only when safe
# (mitcad#1024), in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), as step 7 of tools/ui-sketch-tools-test.sh does on
# Linux, on a design mitcad-cli makes: Sketch1 (F1) on XY with a circle of
# radius 20 about the origin and the points (-30, 50), (-40, -40),
# (40, -40) and (-40, 30).
#   1. A line from (-30, 50) to the circle's top quadrant is held there
#      (coincident with the circle) and plumb with its centre
#      (vertical_points).
#   2. A line between the level points (-40, -40) and (40, -40) is
#      horizontal.
#   3. A line from (-40, 30) clicked near where it touches the circle,
#      (-17.4, -9.9), ends there on the circle and is tangent to it.
#   4. A rectangle drawn afterwards meets no refused constraint and no
#      over-constrained sketch; saved, the file has the coincidents, the
#      alignment, the horizontal and the tangent.
#   5. A right-click while a sketch tool runs is its Esc (mitcad#1085), as
#      step 9 of tools/ui-sketch-tools-test.sh: in a new sketch on XY two
#      Line segments, a right-button drag in the view that does not end
#      the chain, a right-click that ends the chain and keeps the tool, a
#      second one that ends the tool, both without a menu; saved, the new
#      sketch has the two lines.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-auto-constraints-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-auto-constraints'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'auto.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 20},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-30, 50]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-40, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [40, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-40, 30]}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Expect-NewMatch "regex" "description": a line logged since the mark
# matches the regular expression (after an Ui-ExpectNew that waited).
function Expect-NewMatch([string]$Pattern, [string]$Description) {
  $line = Ui-NewLines | Where-Object { $_ -match $Pattern } | Select-Object -First 1
  if (-not $line) { Ui-Fail "${Description}: no line since the mark matches '$Pattern'" }
  Write-Host "ok   ${Description}"
}

Ui-StartApp 'sketch-auto-constraints' @('--open', $design)
Ui-ExpectLog 'auto.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800

Write-Host "--- A line to the circle's top quadrant: held there, plumb with the centre"
Ui-Mark
Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
Ui-ExpectNew 'Sketch tool Line' 'the line tool is active'
Ui-Step 'from the point (-30, 50)'       { Ui-SketchClick -30 50 }
Ui-Step "to the circle's top quadrant"   { Ui-SketchClick 0 20.3 }
Ui-ExpectNew 'from (-30, 50) to (0, 20)' 'the line ends at the top quadrant'
Ui-ExpectNew '(quadrant)' 'the quadrant adds its constraints'
Expect-NewMatch 'Constraint added automatically: coincident p\d+ on c\d+ \(quadrant\)' `
  'the quadrant holds the end on the circle'
Expect-NewMatch 'Constraint added automatically: vertical_points p\d+ p\d+ \(quadrant\)' 'and plumb with its centre'
Ui-Step 'end the chain (Esc)'            { Ui-Key 'Escape' }

Write-Host '--- A line between level points: horizontal'
Ui-Mark
Ui-Step 'from the point (-40, -40)'      { Ui-SketchClick -40 -40 }
Ui-Step 'to the level point (40, -40)'   { Ui-SketchClick 40 -40 }
Ui-ExpectNew 'from (-40, -40) to (40, -40) (horizontal)' 'the line between level points'
Expect-NewMatch 'Constraint added automatically: horizontal c\d+' 'is horizontal'
Ui-Step 'end the chain (Esc)'            { Ui-Key 'Escape' }

Write-Host '--- A line ending where it touches the circle: tangent'
Ui-Mark
Ui-Step 'from the point (-40, 30)'       { Ui-SketchClick -40 30 }
Ui-Step 'near where it touches the circle' { Ui-SketchClick -17.4 -9.9 }
Ui-ExpectNew 'from (-40, 30) to (-17.39818' 'the end goes to the touching point'
Expect-NewMatch 'from \(-40, 30\) to \(-17\.39818\d*, -9\.86424\d*\) \(tangent\)' `
  'where the line touches the circle (-17.39818, -9.86424)'
Ui-ExpectNew 'Constraint added automatically: tangent c' 'the line is tangent to the circle'
Ui-Step 'end the chain (Esc)'            { Ui-Key 'Escape' }
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }

Write-Host '--- A rectangle afterwards: nothing refused, nothing over-constrained'
Ui-Mark
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner'                   { Ui-SketchClick 25 -10 }
Ui-Step 'opposite corner'                { Ui-SketchClick 45 -30 }
Ui-ExpectNew 'Added rectangle' 'a rectangle afterwards'
Ui-Sync
$refused = Ui-NewLines | Where-Object { $_ -match 'not added|over-constrain' } | Select-Object -First 1
if ($refused) { Ui-Fail "the rectangle met a refused constraint or an over-constrained sketch: '$refused'" }
Write-Host 'ok   no refused constraint, no over-constrained sketch'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'

Write-Host '--- A right-click ends the line chain, a second one the tool (mitcad#1085)'
# No-NewLine "text" "description": no line logged since the mark has the text.
function No-NewLine([string]$Text, [string]$Description) {
  Ui-Sync
  if (@(Ui-NewLines | Where-Object { $_.Contains($Text) }).Count -gt 0) { Ui-Fail "${Description}: '$Text' was logged" }
  Write-Host "ok   ${Description}"
}
Ui-Step 'create sketch on XY (F7)'       { Ui-CreateSketch 'F7' 'xy' }
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800
Ui-Mark
Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
Ui-ExpectNew 'Sketch tool Line' 'the line tool is active'
Ui-Mark
Ui-Step 'start a chain'                  { Ui-SketchClick -30 20 }
Ui-Step "the first segment's end"        { Ui-SketchClick -10 20 }
Ui-Step "the second segment's end"       { Ui-SketchClick -10 45 }
Ui-ExpectNew 'Added line c' 'a line is added'
Ui-Sync
$drawn = @(Ui-NewLines | Where-Object { $_.Contains('Added line c') }).Count
if ($drawn -ne 2) { Ui-Fail "the chain added $drawn lines, expected 2" }
Write-Host 'ok   two segments drawn'
# A right-button drag is the view's, not a click: the chain goes on.
Ui-Mark
$from = Ui-ViewAt 30 70
$to = Ui-ViewAt 45 70
Ui-Step 'right-button drag in the view'  { Ui-Drag $from[0] $from[1] $to[0] $to[1] 3 }
No-NewLine 'Line chain ended' 'the right drag did not end the chain'
No-NewLine 'Sketch right-click' 'the right drag is not a right-click'
Ui-Mark
Ui-Step 'right-click in the view'        { Ui-ViewClick 45 60 3 }
Ui-ExpectNew 'Sketch right-click: Esc of the tool' "the right-click is the tool's Esc"
Ui-ExpectNew 'Line chain ended' 'the right-click ended the chain'
No-NewLine 'Sketch tool ended' 'the Line tool stays on'
No-NewLine 'Context menu: ' 'no menu opened'
Ui-Mark
Ui-Step 'right-click again'              { Ui-ViewClick 60 60 3 }
Ui-ExpectNew 'Sketch tool ended: Line' 'the second right-click left the tool'
No-NewLine 'Context menu: ' 'no menu opened'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'the second sketch finished'

Write-Host '--- The saved design'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$sketch = (Get-Content -Raw $design | ConvertFrom-Json).features[0]
$circle = @($sketch.entities | Where-Object { $_.type -eq 'circle' })[0]
$constraints = @($sketch.constraints)
$held = @($constraints | Where-Object { $_.type -eq 'coincident' -and $_.entity -eq $circle.id }).Count
if ($held -ne 2) { Ui-Fail "$held line ends held on the circle, expected 2" }
Write-Host 'ok   two line ends are held on the circle'
$plumb = @($constraints | Where-Object {
    $_.type -eq 'vertical_points' -and ($_.a -eq $circle.center -or $_.b -eq $circle.center) }).Count
if ($plumb -ne 1) { Ui-Fail "$plumb points plumb with the circle's centre, expected 1" }
Write-Host "ok   the quadrant's end is plumb with the circle's centre"
$level = @($sketch.entities | Where-Object { $_.type -eq 'point' -and [double]$_.at[1] -eq -40 } |
  ForEach-Object { $_.id })
$bottom = @($sketch.entities | Where-Object {
    $_.type -eq 'line' -and $level -contains $_.start -and $level -contains $_.end } | ForEach-Object { $_.id })
$horizontal = @($constraints | Where-Object { $_.type -eq 'horizontal' -and $bottom -contains $_.line }).Count
if ($horizontal -ne 1) { Ui-Fail "the line between the level points has $horizontal horizontal constraints, expected 1" }
Write-Host 'ok   the line between the level points is horizontal'
$tangents = @($constraints | Where-Object { $_.type -eq 'tangent' -and ($_.a -eq $circle.id -or $_.b -eq $circle.id) }).Count
if ($tangents -ne 1) { Ui-Fail "$tangents lines tangent to the circle, expected 1" }
Write-Host 'ok   one line is tangent to the circle'
# The second sketch (mitcad#1085): the two segments drawn before the
# right-clicks, nothing more.
$second = @((Get-Content -Raw $design | ConvertFrom-Json).features)[-1]
# A sketch axis's fixed line (mitcad#500, 'axis' set) is not one of them.
$lines = @($second.entities | Where-Object { $_.type -eq 'line' -and -not $_.axis }).Count
if ($lines -ne 2) { Ui-Fail "the second sketch has $lines lines, expected 2" }
Write-Host 'ok   the second sketch has the two lines drawn before the right-clicks'
if (@(Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }).Count -gt 0) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows sketch auto constraints test'
