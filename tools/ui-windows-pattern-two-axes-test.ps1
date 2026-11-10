# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PatternCommands.cpp app/commands/PatternOptions.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/pattern.rs tools/cli
# Input-driven Windows UI test of Circular Pattern's Axis 2 (mitcad#1588)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# as tools/ui-pattern-two-axes-test.sh does on Linux, on a design
# mitcad-cli makes: a 10 mm cube with its corner at (20, 20, 0) (Body1,
# F2.b0; its centre (25, 25, 5)).
#   1. Circular Pattern of Body1 about X, 3 over 180 degrees, Symmetric;
#      Axis 2's rows hidden until its axis is picked, then about Z, 4 over
#      360 degrees; the preview's 12 elements; the added line names both
#      axes.
#   2. Saved; mitcad-cli reads 12 bodies and the copies' centres (element
#      e = k1 + 3 k2, body F3.b(e-1); k1 about X: 0, 90 and -90 degrees,
#      k2 about Z).
#   3. Opened again: the pattern's panel shows Axis 2 with its rows.
#   4. The cubic lattice of tools/cli/tests/f4_pattern_lattice.json (one
#      revolve with the centre ball, one pipe, one pattern about X and Z, a
#      fillet at every joint): every feature opens in its panel and is left
#      as it was.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-pattern-two-axes-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-pattern-two-axes'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.circular_pattern' = 'F8' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'pattern-two-axes.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [20, 20], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

$lattice = Join-Path $work 'lattice.mitcad'
$made = & $cli run (Join-Path $PSScriptRoot 'cli\tests\f4_pattern_lattice.json') --save $lattice
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $lattice)) { Ui-Fail "mitcad-cli (the lattice): $($made -join ' / ')" }

# Expect-Hidden "Panel <command> input <id>" "description": the input's
# last logged place says it is hidden (the panel logs its places again when
# its rows move).
function Expect-Hidden([string]$Text, [string]$Description) {
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-LogLines | Where-Object { $_.Contains("$Text at ") } | Select-Object -Last 1
    if ("$line".EndsWith('(hidden)')) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the last place of '$Text' is not hidden"
}

# Expect-Shown "Panel <command> input <id>" "description": the input's last
# logged place says it is shown.
function Expect-Shown([string]$Text, [string]$Description) {
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-LogLines | Where-Object { $_.Contains("$Text at ") } | Select-Object -Last 1
    if ("$line" -match '\d$') { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the last place of '$Text' is not shown"
}

# Show-Axes "description": the view zoomed out about the cube, a wheel step
# at a time, until the origin axes' pick places lie in the free part of the
# 1024x768 window (as tools/ui-windows-pattern-options-test.ps1 does: the
# view fitted to the 10 mm cube shows the origin's axes beyond the window).
function Show-Axes([string]$Description) {
  # The axis input shows the origin only as its light bulb does (mitcad#1030).
  Ui-ShowOrigin
  $cube = Ui-LoggedAt 'Body F2.b0'
  for ($i = 0; $i -le 24; $i++) {
    $inside = $true
    foreach ($axis in @('x', 'y', 'z')) {
      $line = Ui-LogLines | Where-Object { $_.StartsWith("Datum $axis at ") } | Select-Object -Last 1
      $match = [regex]::Match("$line", ' at (-?\d+),(-?\d+)$')
      if (-not $match.Success) { $inside = $false; continue }
      $at = @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value)
      if ($at[0] -lt 330 -or $at[0] -gt 680 -or $at[1] -lt 180 -or $at[1] -gt 640) { $inside = $false }
    }
    if ($inside) { Write-Host "ok   $Description"; return }
    $before = @(Ui-LogLines | Where-Object { $_.StartsWith('Datum x at ') }).Count
    [UiInput]::Move($script:UiWindow, $cube[0], $cube[1], 0x04)
    Start-Sleep -Milliseconds 80
    [UiInput]::Wheel($script:UiWindow, $cube[0], $cube[1], -1)
    for ($j = 0; $j -lt 20; $j++) {
      Start-Sleep -Milliseconds 200
      if (@(Ui-LogLines | Where-Object { $_.StartsWith('Datum x at ') }).Count -gt $before) { break }
    }
    Ui-Sync
  }
  Ui-Fail "${Description}: the origin axes' places stayed outside the view"
}

# Opens-In-Panel feature uid command: the timeline's feature opens in the
# command's panel, and Esc leaves it as it was. (Not `$Name`: Ui-Step's
# action runs below Ui-Step's scope and would read Ui-Step's own `$Name`,
# the step's text.)
function Opens-In-Panel([string]$Feature, [string]$Uid, [string]$Command) {
  $cell = "Timeline $Feature"
  Ui-Mark
  Ui-Step "edit $Feature"                { Ui-DoubleClickLogged $cell }
  Ui-ExpectNew "Editing $Uid with $Command" "$Feature opens in $Command"
  Ui-Sync
  Ui-Mark
  Ui-Step 'cancel (Esc)'                 { Ui-Key 'Escape' }
  Ui-ExpectNew "Command $Command cancelled" "$Feature left as it was"
}

Ui-StartApp 'pattern-two-axes' @('--open', $design)
Ui-ExpectLog 'pattern-two-axes.mitcad' 'the design opened'
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cube shown'

Write-Host '--- 1. About X, 3 over 180 degrees, symmetric; Axis 2 about Z, 4'
Ui-Mark
Ui-Step 'circular pattern (F8)'          { Ui-Key 'F8' }
Ui-ExpectNew 'Command Circular Pattern started' 'the panel open'
Ui-ExpectNew 'Panel Circular Pattern checks: symmetric=off, symmetric2=off' 'Symmetric 2 among the checks'
Ui-Step 'pick Body1'                     { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew 'Circular Pattern Objects: 1 body [body F2.b0]' 'the cube picked'
Expect-Hidden 'Panel Circular Pattern input quantity2' 'Quantity 2 hidden without a second axis'
Expect-Hidden 'Panel Circular Pattern input angle2' 'Total Angle 2 hidden without a second axis'
Expect-Hidden 'Panel Circular Pattern input symmetric2' 'Symmetric 2 hidden without a second axis'
Ui-Step 'the axis input'                 { Ui-ClickLogged 'Panel Circular Pattern input axis' }
Show-Axes 'the origin axes in the view'
Ui-Step 'pick the X axis'                { Ui-ClickLogged 'Datum x' }
Ui-ExpectNew 'Circular Pattern Axis: 1 axis [axis x]' 'X picked'
Ui-Step 'quantity 3'                     { Ui-TypeIn 'Panel Circular Pattern input quantity' '3' }
Ui-Step 'angle 180'                      { Ui-TypeIn 'Panel Circular Pattern input angle' '180' }
Ui-ExpectNew 'Circular Pattern Total Angle: 180 = 180 deg' 'half a turn'
Ui-Mark
Ui-Step 'Symmetric'                      { Ui-ClickLogged 'Panel Circular Pattern input symmetric' }
Ui-ExpectNew 'Circular Pattern: Symmetric = on' 'symmetric checked'
Ui-ExpectNew 'Preview Circular Pattern elements: 3' '3 elements in the preview' 30
Ui-Mark
Ui-Step 'the second axis input'          { Ui-ClickLogged 'Panel Circular Pattern input axis2' }
Ui-Step 'pick the Z axis'                { Ui-ClickLogged 'Datum z' }
Ui-ExpectNew 'Circular Pattern Axis 2: 1 axis [axis z]' 'Z picked as Axis 2'
Expect-Shown 'Panel Circular Pattern input quantity2' 'Quantity 2 shown'
Expect-Shown 'Panel Circular Pattern input angle2' 'Total Angle 2 shown'
Expect-Shown 'Panel Circular Pattern input symmetric2' 'Symmetric 2 shown'
Ui-Mark
Ui-Step 'quantity 4'                     { Ui-TypeIn 'Panel Circular Pattern input quantity2' '4' }
Ui-ExpectNew 'Circular Pattern Quantity 2: 4' 'four about Z'
Ui-ExpectNew 'Preview Circular Pattern elements: 12' '3 x 4 elements in the preview' 30
Ui-ExpectNew 'Preview Circular Pattern: ok' 'previewed' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added circular pattern of 1 bodies: symmetric, quantity 3, angle 180 deg; axis 2 one direction, quantity 4, angle 360 deg' 'the definition logged' 30
Ui-ExpectNew 'Added Circular Pattern as F3' 'CircularPattern1 added' 30

Write-Host '--- 2. The saved copies'' places'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved' 30
# Element e = k1 + 3 k2 is body F3.b(e-1), the cube's centre (25, 25, 5) a
# quarter turn about X to (25, -5, 25), back to (25, 5, -25); element 3 a
# quarter turn about Z, (-25, 25, 5); 4 both, (5, 25, 25); 11 back about X,
# then three quarters about Z, (5, -25, -25).
$check = Join-Path $work 'check.json'
Set-Content -Encoding ascii $check @'
[
  {"expect": {"bodies": 12}},
  {"expect": {"query": {"query": "feature", "uid": "F3"}, "result": {
    "type": "circular_pattern", "def": {"symmetric": true, "axis2": {"quantity": "d6"}}}}},
  {"expect": {"body": "F3.b0", "center": [25, -5, 25]}},
  {"expect": {"body": "F3.b1", "center": [25, 5, -25]}},
  {"expect": {"body": "F3.b2", "center": [-25, 25, 5]}},
  {"expect": {"body": "F3.b3", "center": [5, 25, 25]}},
  {"expect": {"body": "F3.b10", "center": [5, -25, -25]}}
]
'@
$checked = & $cli run $check --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "the saved design: $($checked -join ' / ')" }
Write-Host 'ok   stored: 12 bodies, the copies about X and Z'

Write-Host '--- 3. Opened again: the panel shows Axis 2'
Ui-StartApp 'pattern-two-axes-reopened' @('--open', $design)
Ui-ExpectLog 'pattern-two-axes.mitcad' 'the design opened again'
Ui-Mark
Ui-Step 'edit CircularPattern1'          { Ui-DoubleClickLogged 'Timeline CircularPattern1' }
Ui-ExpectNew 'Editing F3 with Circular Pattern' 'editing the pattern'
Ui-ExpectNew 'Panel Circular Pattern checks: symmetric=on, symmetric2=off' 'Symmetric checked, Symmetric 2 not'
Expect-Shown 'Panel Circular Pattern input quantity2' 'Quantity 2 shown for the stored second axis'
Ui-ExpectNew 'Preview Circular Pattern elements: 12' 'the stored 12 elements' 30
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Circular Pattern cancelled' 'left as it was'

Write-Host '--- 4. The cubic lattice: every feature opens in its panel'
Ui-StartApp 'pattern-two-axes-lattice' @('--open', $lattice)
Ui-ExpectLog 'lattice.mitcad' 'the lattice opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F3.b0' 'the lattice shown'
Opens-In-Panel 'Plane1' 'F1' 'Plane at Angle'
Opens-In-Panel 'Revolve1' 'F3' 'Revolve'
Opens-In-Panel 'Pipe1' 'F4' 'Pipe'
Ui-Mark
Ui-Step 'edit CircularPattern1'          { Ui-DoubleClickLogged 'Timeline CircularPattern1' }
Ui-ExpectNew 'Editing F5 with Circular Pattern' 'CircularPattern1 opens in Circular Pattern'
Ui-ExpectNew 'Panel Circular Pattern checks: symmetric=on, symmetric2=off' 'the lattice''s pattern: symmetric about X'
Ui-ExpectNew 'Preview Circular Pattern elements: 12' 'the lattice''s pattern previews its 12 elements' 60
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Circular Pattern cancelled' 'CircularPattern1 left as it was'
Opens-In-Panel 'Fillet1' 'F6' 'Fillet'
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows pattern two axes test'
