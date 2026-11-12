# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view app/sketch tools/cli
# Input-driven Windows UI test of the view cube in a sketch with a
# constraint selected (mitcad#773: a click on a cube arrow or Home ended the
# application with an exception out of the button's slot), in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-sketch-cube-test.sh on Linux:
#   1. A design (mitcad-cli): Sketch1 on XY with a line c2 from the origin
#      to (30, 20) whose horizontal constraint k1 is inactive, as an import
#      leaves one the file does not meet, and a line from (40, 0) to
#      (40, 25) with an active vertical constraint k2. The sketch opens
#      looking at XY, face on: the cube's arrows show.
#   2. With the active k2 selected (a click on its symbol), each arrow
#      (up, down, clockwise, counter-clockwise, right, left) turns the view
#      to the face beside or rolls it, and Home turns it to the home view.
#   3. The same with the inactive k1 selected, after a click on the cube's
#      top face turned the view back to the sketch.
#   4. A new sketch on XY drawn here: a horizontal line, whose inferred
#      horizontal constraint is selected; every arrow and Home again.
# After each click the application lives on, wrote no crash report
# (MITCAD_CRASH_DIR) and logged no failed view operation ("View operation
# failed: ...", which the view logs instead of letting an exception end the
# application).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-cube-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-cube'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$script = Join-Path $work 'cube.json'
$design = Join-Path $work 'cube.mitcad'
# The horizontal constraint made first, then set aside: the line's end is
# dragged back up (as tools/ui-windows-sketch-inactive-test.ps1 does). An
# active constraint has no "active" in the answer.
Set-Content -Encoding ascii $script @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [0, 0], "fixed": true},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": "p1", "end": [30, 20]},
  {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "horizontal", "line": "c2"}},
  {"cmd": "sketch.set_active", "sketch": "F1", "items": ["k1"], "active": false,
   "reason": "not met in the imported file"},
  {"cmd": "sketch.drag", "sketch": "F1", "entity": "p3", "to": [30, 20]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [40, 0], "end": [40, 25]},
  {"cmd": "sketch.add_constraint", "sketch": "F1", "constraint": {"type": "vertical", "line": "c4"}},
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "constraints": [{"id": "k1", "type": "horizontal", "active": false},
                    {"id": "k2", "type": "vertical", "line": "c4"}],
    "unmet_inactive": ["k1"]}}}
]
'@
$made = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Where a click hits the cube's faces (the top face, to turn back to the
# sketch), logged as the camera rests.
$env:MITCAD_LOG_ORIENTATION_CUBE = '1'
# Ui-StartApp's crash folder for the app: an unhandled exception writes
# its report there before the process ends (mitcad#76, mitcad#773).
$crashes = Join-Path $script:UiOut 'crashes'

# Still-Alive "description": the app runs, wrote no crash report and logged
# no failed view operation since the mark.
function Still-Alive([string]$Description) {
  if (Ui-Crashed) { Ui-Fail "${Description}: the application ended (exit code $($script:UiProcess.ExitCode))" }
  $report = Get-ChildItem -Path $crashes -Filter 'crash-*.crash' -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($report) {
    $message = Get-Content $report.FullName | Where-Object { $_.StartsWith('message: ') } | Select-Object -First 1
    Ui-Fail "${Description}: a crash report was written ($($report.Name): $message)"
  }
  $failed = Ui-LogLines | Select-Object -Skip $script:UiMark |
    Where-Object { $_.Contains('View operation failed') -or $_.Contains('OCCT view event failed') } | Select-Object -First 1
  if ($failed) { Ui-Fail "${Description}: $failed" }
}

# Expect-Camera "direction ... up ..." "description": the camera at rest
# after the action since the mark.
function Expect-Camera([string]$Camera, [string]$Description) {
  Ui-ExpectNew "Camera direction $Camera" $Description 8
}

# Turn "arrow" "expected camera" "description": a click on the cube's arrow.
function Turn([string]$Arrow, [string]$Camera, [string]$Description) {
  Ui-Mark
  Ui-Step "the cube's arrow $Arrow"        { Ui-ClickLogged "Orientation cube arrow $Arrow" }
  Expect-Camera $Camera $Description
  Still-Alive "the arrow $Arrow"
}

# Arrows-And-Home "constraint": every arrow, then Home, with the constraint
# selected while the sketch is edited; the sketch stays edited.
function Arrows-And-Home([string]$Constraint) {
  $from = @(Ui-LogLines).Count
  Ui-ExpectLog 'Orientation cube arrow up at' "the cube's arrows show face on"
  Turn 'up' '0 -1 0 up 0 0 -1' 'up: turned to the face above'
  Turn 'down' '0 0 -1 up 0 1 0' 'down: back to the sketch'
  Turn 'cw' '0 0 -1 up -1 0 0' 'clockwise: rolled 90 degrees'
  Turn 'ccw' '0 0 -1 up 0 1 0' 'counter-clockwise: rolled back'
  Turn 'right' '-1 0 0 up 0 1 0' 'right: turned to the face on the right'
  Turn 'left' '0 0 -1 up 0 1 0' 'left: back to the sketch'
  Ui-Mark
  Ui-Step 'Home'                           { Ui-ClickLogged 'View home' }
  Expect-Camera '-0.568 0.654 -0.5 up' 'Home: the home view (the default view, mitcad#1698)'
  Still-Alive 'Home'
  if (Ui-LogLines | Select-Object -Skip $from | Where-Object { $_.Contains('Sketch finished') }) {
    Ui-Fail 'the sketch was finished by a view turn'
  }
  Write-Host "ok   $Constraint selected: every arrow and Home turned the view, the sketch still edited"
}

# Select-Constraint "k": a click on its symbol selects it.
function Select-Constraint([string]$Constraint) {
  Ui-ExpectLog "Constraint $Constraint glyph at" "$Constraint's symbol is placed"
  Start-Sleep -Milliseconds 800
  Ui-Mark
  Ui-Step "select $Constraint (its symbol)" { Ui-ClickLogged "Constraint $Constraint glyph" }
  Ui-ExpectNew 'Selected: 1 constraint' "$Constraint selected"
}

Ui-StartApp 'cube' @('--open', $design)
Ui-ExpectLog 'cube.mitcad' 'the design opened'
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-ExpectNew 'Sketch inactive drawn: k1 constraint' 'k1 is drawn inactive'
Expect-Camera '0 0 -1 up 0 1 0' 'looking at the sketch'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }

Write-Host '--- 1. The active constraint k2 selected'
Select-Constraint 'k2'
Arrows-And-Home 'k2'

Write-Host '--- 2. The inactive constraint k1 selected'
Ui-ExpectLog 'Orientation cube top at' "the cube's top face is placed"
Ui-Mark
Ui-Step "the cube's top face"            { Ui-ClickLogged 'Orientation cube top' }
Expect-Camera '0 0 -1 up 0 1 0' 'looking at the sketch again'
Still-Alive "the cube's top face"
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Select-Constraint 'k1'
Arrows-And-Home 'k1'

Write-Host '--- 3. A sketch drawn here: a horizontal line, its constraint selected'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch1 finished'
Ui-CreateSketch 'F7' 'xy'
Ui-ExpectNew 'Sketch view' 'the new sketch is in sight'
Expect-Camera '0 0 -1 up 0 1 0' 'looking at the new sketch'
Ui-Mark
Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
Ui-ExpectNew 'Sketch tool Line' 'line tool active'
Ui-Step 'line: start (-10, -5)'          { Ui-SketchClick -10 -5 }
Ui-Step 'line: end (10, -5)'             { Ui-SketchClick 10 -5 }
Ui-ExpectNew ') (horizontal)' 'the line drawn, its horizontal constraint inferred'
Ui-Step 'end the tool (Esc Esc)'         { Ui-Key 'Escape'; Ui-Key 'Escape' }
# Its constraint: the symbol placed since the line tool started (the new
# sketch has only it; Sketch1's symbols were placed before).
$drawn = $null
for ($i = 0; $i -lt 30 -and -not $drawn; $i++) {
  $line = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_ -match '^Constraint k[0-9]+ glyph at ' } |
    Select-Object -Last 1
  if ("$line" -match '^Constraint (k[0-9]+) glyph at ') { $drawn = $Matches[1] } else { Start-Sleep -Milliseconds 200 }
}
if (-not $drawn) { Ui-Fail "the drawn line's constraint symbol was not placed" }
Select-Constraint $drawn
Arrows-And-Home $drawn

Ui-Finish 'Windows UI sketch view cube test'
