# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/ManipulatorOverlay.hpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/CommandSession.cpp tools/cli
# Input-driven Windows UI test of a click on a manipulator's knob drawn
# over the orientation cube (mitcad#1105) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-knob-over-cube-test.sh on Linux: a press that starts on a
# handle stays with it even without a drag, so it never reaches the cube
# (or the view) as a click. On a 40 mm cube that mitcad-cli makes (Box1,
# F1, body F1.b0), seen from the home view: Fillet on the top front edge
# shows its Radius arrow; the view is panned (middle drag) until the
# arrow's grab point lies on the place where a click turns the view to the
# cube's top face, and that place is clicked without a drag:
#   - the handle keeps the click ("Manipulator Fillet sets.0.radius
#     clicked without a drag: kept by the handle"), the radius is not
#     dragged, and the logged camera is unchanged (the view did not turn);
#   - with Fillet cancelled (no handle), the same place of the cube turns
#     the view to the top, as before: other clicks are unchanged.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-knob-over-cube-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-knob-over-cube'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'cube.json'
$design = Join-Path $work 'cube.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Where a click hits the cube's faces, logged as the camera rests (and
# again when the cube moves with the panels).
$env:MITCAD_LOG_ORIENTATION_CUBE = '1'
$edge = 'E{F1:side0|F1:top}'
$knob = 'Manipulator Fillet sets.0.radius'
$cameraPattern = 'Camera direction [^(]*\([a-z]*\)'

# Place "text": x, y (window pixels) of the newest "<text> at x,y" line;
# $null for none.
function Place([string]$Text) {
  $line = Ui-LastLineWith "$Text at "
  $match = [regex]::Match("$line", ' at (-?\d+),(-?\d+)')
  if (-not $match.Success) { return $null }
  return @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value)
}

# Camera: the newest logged camera (direction, up and projection).
function Camera {
  $found = [regex]::Matches("$(Ui-LogText)", $cameraPattern)
  if ($found.Count -eq 0) { return $null }
  return $found[$found.Count - 1].Value
}

# Settle: the app idle, its places logged, the camera at rest.
function Settle {
  Ui-WaitIdle
  Start-Sleep -Milliseconds 800
  Ui-Sync
}

# Knob-Onto-Cube: pans (middle drags from the knob) until the knob's grab
# point is within 3 pixels of the place where a click turns the view to
# the cube's top face.
function Knob-Onto-Cube {
  for ($round = 1; $round -le 4; $round++) {
    $cube = Place 'Orientation cube top'
    $at = Place $knob
    if (-not $cube -or -not $at) { Ui-Fail "no places of the cube's top face and the knob logged" }
    $dx = $cube[0] - $at[0]
    $dy = $cube[1] - $at[1]
    if ($dx * $dx + $dy * $dy -le 9) {
      Write-Host "ok   the knob at $($at[0]),$($at[1]) over the cube's top face at $($cube[0]),$($cube[1])"
      return
    }
    Ui-Mark
    Ui-Step "pan the knob by $dx,$dy (round $round)" { Ui-Drag $at[0] $at[1] ($at[0] + $dx) ($at[1] + $dy) 2 }
    Ui-ExpectNew "$knob at " 'the knob moved with the view'
    Settle
  }
  Ui-Fail "the knob at $($at[0]),$($at[1]) did not come onto the cube's top face at $($cube[0]),$($cube[1])"
}

Ui-StartApp 'knob' @('--open', $design)
Ui-ExpectLog 'cube.mitcad' 'the design opened'
Ui-Step 'the home view'                  { Ui-ClickLogged 'View home' }
Ui-ExpectLog 'Camera direction -0.577 0.577 -0.577' 'seen from the front, the right and above'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Settle

Write-Host "--- Fillet's arrow moved onto the cube"
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectLog 'Fillet Radius: 2 mm' 'Fillet started' 30
Ui-Mark
Ui-Step 'pick the top front edge'        { Ui-ClickPick "edge F1.b0/$edge" }
Ui-ExpectNew 'Fillet Edges: 1 edge' 'the edge picked'
Ui-ExpectNew "$knob at " 'Radius has an arrow'
Ui-ExpectNew 'Preview Fillet: ok' 'previewed' 30
Settle
Knob-Onto-Cube

Write-Host '--- The click stays with the knob'
$before = Camera
if (-not $before) { Ui-Fail 'no camera logged' }
$cube = Place 'Orientation cube top'
Ui-Mark
Ui-Step 'click the knob over the cube'   { Ui-ClickLogged 'Orientation cube top' }
Ui-ExpectNew "$knob clicked without a drag: kept by the handle" 'the knob kept the click'
Settle
Start-Sleep -Milliseconds 1000
Ui-Sync
$now = Place 'Orientation cube top'
if (-not $now -or $now[0] -ne $cube[0] -or $now[1] -ne $cube[1]) { Ui-Fail 'the cube moved before the click' }
foreach ($line in @(Ui-NewLines)) {
  if ($line.Contains("$knob dragged to")) { Ui-Fail 'the click dragged the radius' }
  $match = [regex]::Match($line, $cameraPattern)
  if ($match.Success -and $match.Value -ne $before) { Ui-Fail "the view turned: $before -> $($match.Value)" }
}
Write-Host "ok   the camera unchanged: $before"
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- Without the handle the cube still turns the view'
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Fillet cancelled' 'the fillet cancelled' 30
Settle
Ui-Mark
Ui-Step "the cube's top face"            { Ui-ClickLogged 'Orientation cube top' }
Ui-ExpectNew 'Camera direction 0 0 -1 ' 'the view turned to the top' 8

Ui-Finish 'Windows UI knob over the orientation cube test'
