# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework app/MainWindow.cpp app/commands/SketchCommands.cpp tools/cli
# Input-driven Windows UI test of the Sketch Palette's Create section and
# its constraints on two rows (mitcad#1027), in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as step 8 of
# tools/ui-sketch-tools-test.sh does on Linux, on a design mitcad-cli
# makes: Sketch1 (F1) on XY with a circle of radius 20 about the origin and
# the points (-50, -40) and (50, 40). The Ellipse has F8 (the command
# search's popup closes at once in session 0).
#   1. The palette opened before any tool started shows the defaults (Line,
#      Rectangle, Circle, Arc, Polygon, Slot, Spline, Point, Mirror,
#      Dimension, Fillet, Trim, Extend, Offset: the ribbon's order); the
#      Constraints take two rows of seven, Create two rows, and no button
#      is clipped.
#   2. The Ellipse (not a default), started three times, drawing an ellipse
#      each time, does not move the buttons during the sketch.
#   3. Sketch1 opened again: the Ellipse is in the section (counts); its
#      Create button starts the tool and shows checked.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-palette-create-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-palette-create'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.ellipse' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'palette.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.circle", "sketch": "F1", "mode": "center", "center": [0, 0], "radius": 20},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-50, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [50, 40]}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Create-Line: the last "Palette create tools" line since the mark.
function Create-Line {
  $line = Ui-NewLines | Where-Object { $_.StartsWith('Palette create tools: ') } | Select-Object -Last 1
  if (-not $line) { Ui-Fail 'no Palette create tools line since the mark' }
  return "$line"
}

# Create-Has "line" "tool": the line names the tool (ids without "sketch.").
function Create-Has([string]$Line, [string]$Tool) {
  return $Line -match "(: |, )$([regex]::Escape($Tool))(,| \()"
}

$defaults = 'line, point, rectangle, circle, arc, polygon, slot, spline, mirror, dimension, fillet, trim, extend, offset'

Ui-StartApp 'sketch-palette-create' @('--open', $design)
Ui-ExpectLog 'palette.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }

Write-Host '--- 1. The defaults; the constraints on two rows; nothing clipped'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-ExpectNew 'Palette create tools: ' 'the palette chose its Create tools as it opened'
$first = Create-Line
if ($first -ne "Palette create tools: $defaults (defaults)") {
  Ui-Fail "the Create section is '$first', expected the defaults ($defaults)"
}
Write-Host 'ok   the palette opened before any tool started shows the defaults'
Ui-ExpectNew 'Palette Constraints: 13 buttons in 2 rows of 7' 'the constraints on two rows of seven'
Ui-ExpectNew 'Palette Create: 14 buttons in 2 rows of 7' "the Create section's two rows of seven"
Ui-ExpectNew 'Palette buttons clipped: none' 'no button is clipped: the palette keeps its width'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800

Write-Host '--- 2. The Ellipse started three times; the buttons stay'
Ui-Mark
$places = @(@(-30, 25), @(30, -25), @(-30, -25))
foreach ($at in $places) {
  $x = $at[0]
  $y = $at[1]
  Ui-Step 'ellipse (F8)'                 { Ui-Key 'F8' }
  Ui-ExpectNew 'Sketch tool Ellipse' 'the Ellipse tool is active'
  Ui-Step "centre ($x, $y)"              { Ui-SketchClick $x $y }
  Ui-Step 'end of the major axis'        { Ui-SketchClick ($x + 10) $y }
  Ui-Step 'a point on the ellipse'       { Ui-SketchClick $x ($y + 4) }
  Ui-ExpectNew 'Added ellipse c' "an ellipse at ($x, $y)"
  Ui-Step 'end the tool (Esc)'           { Ui-Key 'Escape' }
}
Ui-Sync
if (@(Ui-NewLines | Where-Object { $_.StartsWith('Palette create tools: ') }).Count -gt 0) {
  Ui-Fail 'the Create section was chosen again during the sketch'
}
Write-Host 'ok   the Create buttons stay as they are during the sketch'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'

Write-Host '--- 3. Opened again: the Ellipse in the section; its button starts it'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited again' 30
Ui-ExpectNew 'Palette create tools: ' 'the Create tools chosen again'
$again = Create-Line
if (-not (Create-Has $again 'ellipse') -or -not $again.EndsWith(' (counts)')) {
  Ui-Fail "the Ellipse started three times is not in '$again'"
}
Write-Host 'ok   the Ellipse started three times is in the section when the sketch opens again'
Ui-ExpectNew 'Palette paletteCreate_sketch.ellipse at' 'its Create button is placed'
Ui-Mark
Ui-Step "the Ellipse's Create button"    { Ui-ClickLogged 'Palette paletteCreate_sketch.ellipse' }
Ui-ExpectNew 'Sketch tool Ellipse' 'the Create button starts its tool'
Ui-ExpectNew 'Palette create checked: ellipse' 'its button shows checked while the tool is on'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished again'
if (@(Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }).Count -gt 0) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows sketch palette create test'
