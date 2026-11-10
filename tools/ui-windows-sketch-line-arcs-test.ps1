# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp tools/cli
# check-all sources: core/sketch core/model-api/src/sketch.rs core/solver
# Input-driven Windows UI test of the Line tool's arcs and the Midpoint Line
# (mitcad#1393) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), as tools/ui-sketch-line-arcs-test.sh does on Linux, on a design
# mitcad-cli makes: Sketch1 (F1) on XY with a line c1 from p2 (-40, -20) to
# p3 (-10, -20), points p4 (-60, 40) and p5 (60, 40) that widen the fitted
# view, p6 (20, 20) for a midpoint, p7 (30, -40) and p8 (50, -40) for a chain,
# p9 (-40, 10) and p10 (-20, 10) for another.
#   1. The Line tool pressed on c1's end p3 and dragged along it draws a
#      tangent arc from c1 (no chain before it).
#   2. A chain's line from p7 to p8, then a drag from its end across it,
#      with 15 typed while the button is down: a normal arc with a radius
#      dimension of 15 mm.
#   2b. A chain's line from (-40, 10) to (-20, 10), then a drag from its
#      end up across it with 10, Tab and 270 typed: the normal arc leaves
#      upward, toward the pointer, and sweeps 270 degrees round (-10, 10)
#      to (-10, 0), below the line (mitcad#1608).
#   3. Midpoint Line: a click on p6 and one at (5, 10): the line's middle
#      is p6, its ends as far on either side.
#   4. Saved, the file has the normal arc's radius dimension and the
#      midpoint constraint.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-line-arcs-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-line-arcs'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.line_midpoint' = 'F7' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'line-arcs.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [-40, -20], "end": [-10, -20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-60, 40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [60, 40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [20, 20]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [30, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [50, -40]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-40, 10]},
  {"cmd": "sketch.add_point", "sketch": "F1", "at": [-20, 10]}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Arc-Drag x1 y1 x2 y2 [text] [sweep]: a left drag between sketch points
# (mm) in eight steps that types the text halfway, with the button still
# down, and then Tab and the sweep.
function Arc-Drag([double]$X1, [double]$Y1, [double]$X2, [double]$Y2, [string]$Text = '', [string]$Sweep = '') {
  $from = Ui-SketchAt $X1 $Y1
  $to = Ui-SketchAt $X2 $Y2
  $b = $script:UiButtons[1]
  Ui-Apart $script:UiWindow $from[0] $from[1]
  [UiInput]::Move($script:UiWindow, $from[0], $from[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($script:UiWindow, $b[0], $from[0], $from[1], $b[3])
  for ($i = 1; $i -le 8; $i++) {
    Start-Sleep -Milliseconds 80
    [UiInput]::Move($script:UiWindow, [int]($from[0] + ($to[0] - $from[0]) * $i / 8),
                    [int]($from[1] + ($to[1] - $from[1]) * $i / 8), $b[3])
    if ($i -eq 4 -and $Text) {
      Ui-SyncOr 300
      Ui-Type $Text
      Ui-SyncOr 300
      if ($Sweep) {
        Ui-Key 'Tab'
        Ui-SyncOr 300
        Ui-Type $Sweep
        Ui-SyncOr 300
      }
    }
  }
  Ui-SyncOr 300
  [UiInput]::Button($script:UiWindow, $b[1], $to[0], $to[1], 0)
}

Ui-StartApp 'line-arcs' @('--open', $design)
Ui-ExpectLog 'line-arcs.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'edit Sketch1 (double-click)'    { Ui-DoubleClickLogged 'Timeline Sketch1' }
Ui-ExpectNew 'Editing sketch F1 (Sketch1)' 'the sketch is edited' 30
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Start-Sleep -Milliseconds 800

Write-Host '--- A tangent arc from the end of a line'
Ui-Mark
Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
Ui-Step "drag from c1's end along it"    { Arc-Drag -10 -20 10 -10 }
Ui-ExpectNew 'Line: an arc from the end p3 of c1' "the press on c1's end starts an arc from c1"
Ui-ExpectNew 'Line arc: tangent' 'a drag along the line chooses a tangent arc'
Ui-ExpectNew 'Added tangent arc c' 'the tangent arc is added'
Ui-Step 'end the chain (Esc)'            { Ui-Key 'Escape' }
Ui-ExpectNew 'Line chain ended' 'the chain ended'

Write-Host "--- A normal arc with a typed radius from a chain's line"
Ui-Mark
Ui-Step 'chain: the start on p7'         { Ui-SketchClick 30 -40 }
Ui-Step 'chain: the end on p8'           { Ui-SketchClick 50 -40 }
Ui-ExpectNew 'Added line c' "the chain's line"
Ui-ExpectNew ' from (30, -40) to (50, -40)' 'from p7 to p8'
Ui-Step 'drag across it, typing 15'      { Arc-Drag 50 -40 60 -25 '15' }
Ui-ExpectNew 'Line arc: normal' 'a drag across the line chooses a normal arc'
Ui-ExpectNew 'Added normal arc c' 'the normal arc is added'
Ui-ExpectNew ', radius 15.000 mm, dimensioned' 'the typed radius dimensions it'
Ui-Step 'end the chain (Esc)'            { Ui-Key 'Escape' }
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
if (@(Ui-LogLines | Where-Object { $_.Contains('Sketch command sketch.arc refused') }).Count -gt 0) {
  Ui-Fail 'an arc was refused'
}

Write-Host '--- A normal arc of a typed radius and a sweep of 270 degrees'
Ui-Mark
Ui-Step 'line tool (L)'                  { Ui-Key 'l' }
Ui-Step 'chain: the start on p9'         { Ui-SketchClick -40 10 }
Ui-Step 'chain: the end on p10'          { Ui-SketchClick -20 10 }
Ui-ExpectNew ' from (-40, 10) to (-20, 10)' "the chain's line"
Ui-Step 'drag up across it, typing 10 and 270' { Arc-Drag -20 10 -15 25 '10' '270' }
Ui-ExpectNew 'Line arc: normal' 'a drag across the line chooses a normal arc'
Ui-ExpectNew ', radius 10.000 mm, dimensioned' 'the typed radius dimensions it'
Ui-ExpectNew 'sweeps 270.0 degrees' 'it leaves upward, toward the pointer, and sweeps 270 degrees (mitcad#1608)'
Ui-Step 'end the chain (Esc)'            { Ui-Key 'Escape' }
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
if (@(Ui-LogLines | Where-Object { $_.Contains('Sketch command sketch.arc refused') }).Count -gt 0) {
  Ui-Fail 'an arc was refused'
}

Write-Host '--- Midpoint Line'
Ui-Mark
Ui-Step 'midpoint line (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Sketch tool Midpoint Line' 'Midpoint Line started'
Ui-Step 'the midpoint on p6'             { Ui-SketchClick 20 20 }
# Low in the view: a click at (40, 30) hit the orientation cube in the VM's window.
Ui-Step 'an end'                         { Ui-SketchClick 5 10 }
Ui-ExpectNew 'Added midpoint line c' 'the midpoint line is added'
$pattern = 'Added midpoint line c\d+ from \(([^,]+), ([^)]+)\) to \(([^,]+), ([^)]+)\) about \(([^,]+), ([^)]+)\)'
$line = Ui-LogLines | Where-Object { $_ -match $pattern } | Select-Object -Last 1
if (-not $line -or $line -notmatch $pattern) { Ui-Fail 'no midpoint line logged' }
$n = 1..6 | ForEach-Object { [double]::Parse($Matches[$_], [Globalization.CultureInfo]::InvariantCulture) }
$dx = [Math]::Abs(($n[0] + $n[2]) / 2 - $n[4])
$dy = [Math]::Abs(($n[1] + $n[3]) / 2 - $n[5])
if ($n[4] -ne 20 -or $n[5] -ne 20 -or $dx -gt 1e-5 -or $dy -gt 1e-5) {
  Ui-Fail "the midpoint line runs from ($($n[0]), $($n[1])) to ($($n[2]), $($n[3])) about ($($n[4]), $($n[5])), not about (20, 20)"
}
Write-Host 'ok   its middle is the first click, p6 at (20, 20)'
Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
if (@(Ui-LogLines | Where-Object { $_.Contains('Midpoint Line not added') }).Count -gt 0) {
  Ui-Fail 'the midpoint line was refused'
}

Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$saved = [IO.File]::ReadAllText($design)
if ($saved -notmatch '"type": ?"radius"') { Ui-Fail 'the saved sketch has no radius dimension' }
Write-Host "ok   the saved sketch has the arc's radius dimension"
if ($saved -notmatch '"type": ?"midpoint"') { Ui-Fail 'the saved sketch has no midpoint constraint' }
Write-Host 'ok   the saved sketch has the midpoint constraint'

Ui-Finish 'UI Windows sketch line arcs test'
