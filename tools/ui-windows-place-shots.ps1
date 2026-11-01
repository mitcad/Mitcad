# SPDX-License-Identifier: MIT
# Screenshots of placement by picked geometry (mitcad#656) on the light
# theme, for a reviewer to judge: Place On with its in-view dimensions and
# value boxes, Gear Pair and Belt Drive. Not a test: a shot fails only when
# Mitcad does not get through the steps that lead to it. Runs mitcad.exe
# with Mesa's software OpenGL (MITCAD_MESA_DIR) in SSH's session 0 with the
# in-process test driver (MITCAD_TEST_INPUT, app/framework/TestDriver.hpp),
# as tools/ui-windows-print-settings-shots.ps1 does: `screenshot` grabs the
# main window with the 3D view, its value boxes and the floating panels
# composed in, without the native title bar. Every shot follows an
# expect-log that proves what it shows is there, so a missing step fails
# loudly instead of giving an empty picture. The flows, picks and log lines
# are those of tools/ui-windows-place-test.ps1, ui-windows-drives-test.ps1
# and ui-windows-pulley-test.ps1; the view's picks are clicked where the app
# logs them (MITCAD_LOG_PICKS: "Pick edge F1.b0/E{...} at x,y").
#
# mitcad-cli makes the data first:
#   plate.mitcad   an 80 x 60 x 10 plate (Sketch1 F1, Extrude1 F2.b0) and
#                  Sketch2 (F3) with one point at (50, -20) in front of it
#   shafts.mitcad  two 8 mm shafts 30 high, 60 mm apart (Cylinder1 F1.b0 at
#                  the origin, Cylinder2 F2.b0 at (60, 0))
#   old.mitcad     a spur gear as earlier versions wrote it, by its plane
#                  and centre (Gear1 F1.b0 on XY at (40, 25))
#
# Each group of shots is a run of its own (its own settings, projects and
# autosave folders, a copy of its design), so that a failed step costs
# only that run's shots. Floating chrome unless named:
#   01-box-on-face.png          Box, a click on the plate's top face: the
#                               two dimensions to the nearest edges with
#                               their value boxes, and the panel
#   02-value-box-open.png       Distance 1's value box clicked open for
#                               typing (then 15 typed, Enter; Distance 2
#                               12 in the panel)
#   03-box-angle.png            More opened, Angle 10 in the panel (its
#                               value box hidden at 0), the value box
#                               clicked, 30 typed, Enter: the ring turned
#                               from its zero line along Reference 1 with
#                               its value box, Angle and Align to in
#                               the panel
#   04-pulley-on-shaft.png      Pulley, Place On the first shaft's top edge,
#                               More, Offset 2: the bore follows the shaft
#                               (8.2 mm with the printed fit's 0.2 mm)
#   05-pulley-check-belt.png    after OK, a second pulley (28 teeth) on the
#                               second shaft, Offset 2, Check Belt (right
#                               under Place On) a face of the first: its
#                               rows (belt length, nearest belt, travel)
#                               and the belt's pitch line in the view
#   06-gear-pair-candidates.png Gear Pair, the shafts' top edges as the two
#                               axes, ratio 2 (the default), New Body: the
#                               list of gears that fit with its columns
#   07-gear-pair-preview.png    the second row (module 1.25, 32 : 64)
#                               chosen and previewed
#   08-belt-drive.png           Belt Drive between the same shafts, ratio 2
#                               and GT2 2 mm (the defaults), New Body: the
#                               rows, the first previewed, the belt in the
#                               note
#   09-hole-on-face.png         Hole, a click on the plate's top face,
#                               distances 20 and 15, diameter 8: its two
#                               dimensions
#   10-cylinder-at-point.png    Cylinder, a click on Sketch2's point
#   11-edit-old-design.png      Gear1 of old.mitcad (`plane` and `center`)
#                               opened for editing (a double-click in the
#                               Timeline): its placement as a point on XY
#                               from the origin axes
#   12-docked-gear-pair.png     the Gear Pair of 06 in the Docked chrome
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-place-shots.ps1 -App <mitcad.exe> [-Cli <mitcad-cli.exe>] -Out <dir> [-QtBin dir] [-Scale factor]
# -Cli defaults to the build's tools\cli\mitcad-cli.exe; Qt's DLLs are not
# copied next to the app, -QtBin defaults to the Qt of the build directory's
# CMakeCache.txt. -Scale is Qt's scale factor (QT_SCALE_FACTOR, 0.75 by
# default), as in tools/ui-windows-window-shots.ps1: session 0's screen is
# 1024 x 768, too small for the window's 1280 x 800 logical pixels, which a
# scale of 0.75 makes room for; the images are 0.75 of that in pixels.
#
# Worked around here: a drop-down's list does not open in session 0, so the
# Operation of Gear Pair and Belt Drive is changed by the keyboard (the
# field focused, Down: New Body after New Component, as the tests choose
# it); the driver's clicks do not scroll a long panel, so the Pulley panel
# is scrolled to Check Belt by wheel steps over its scroll bar (see 04,
# 05 below).
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-place-shots'),
  [string]$QtBin = '',
  [double]$Scale = 0.75)

$ErrorActionPreference = 'Stop'
$App = (Resolve-Path $App).Path
New-Item -ItemType Directory -Force $Out | Out-Null
$Out = (Resolve-Path $Out).Path

if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', 'tools\cli\mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $App) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) {
  Write-Host "FAIL: no mitcad-cli.exe beside $App; pass -Cli"
  exit 1
}
$Cli = (Resolve-Path $Cli).Path
if (-not $QtBin) {
  $cache = Join-Path (Split-Path $App) 'CMakeCache.txt'
  $qtDir = if (Test-Path $cache) { Select-String -Path $cache -Pattern '^Qt6_DIR:PATH=(.*)' } else { $null }
  if ($qtDir) { $QtBin = Join-Path $qtDir.Matches[0].Groups[1].Value '..\..\..\bin' }
}
if (-not $QtBin -or -not (Test-Path (Join-Path $QtBin 'Qt6Core.dll'))) {
  Write-Host "FAIL: Qt's bin directory not found; pass -QtBin"
  exit 1
}
$env:Path = (Resolve-Path $QtBin).Path + ';' + $env:Path
if (-not (Test-Path (Join-Path (Split-Path $App) 'opengl32.dll'))) {
  Write-Host "FAIL: no opengl32.dll next to $App. Build Mesa (tools\dev-env\setup-mesa-windows.ps1)"
  Write-Host '      and configure with -DMITCAD_MESA_DIR=C:/dev/mesa/bin.'
  exit 1
}

$shots = @('01-box-on-face.png', '02-value-box-open.png', '03-box-angle.png', '04-pulley-on-shaft.png',
           '05-pulley-check-belt.png', '06-gear-pair-candidates.png', '07-gear-pair-preview.png',
           '08-belt-drive.png', '09-hole-on-face.png', '10-cylinder-at-point.png', '11-edit-old-design.png',
           '12-docked-gear-pair.png')
foreach ($shot in $shots) { Remove-Item -Force (Join-Path $Out $shot) -ErrorAction SilentlyContinue }
$saved = New-Object System.Collections.Generic.List[string]
$failed = New-Object System.Collections.Generic.List[string]

# --- The data, made with mitcad-cli

$work = Join-Path $Out 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# Invoke-Cli name script arguments: `mitcad-cli run` of a JSON script;
# whether it succeeded.
function Invoke-Cli([string]$Name, [string]$Json, [string[]]$Arguments) {
  $file = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $file $Json
  $output = & $Cli run $file @Arguments
  if ($LASTEXITCODE -ne 0) {
    Write-Host "FAIL: mitcad-cli ($Name), exit code ${LASTEXITCODE}:"
    $output | ForEach-Object { Write-Host "     $_" }
    return $false
  }
  return $true
}

Write-Host '--- The data'
# The plate's top face is F2:end(RECT), its sides the rectangle's lines.
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$plate = Join-Path $work 'plate.mitcad'
$plateJson = (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 80, "height": 60},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [50, -20], "fixed": true}
]
'@).Replace('RECT', $rect)
if ((Invoke-Cli 'plate' $plateJson @('--save', $plate)) -and (Test-Path $plate)) { Write-Host 'ok   the plate' }

$shafts = Join-Path $work 'shafts.mitcad'
$shaftsJson = @'
[
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [60, 0],
    "diameter": 8, "height": 30, "operation": "new_body"}}
]
'@
if ((Invoke-Cli 'shafts' $shaftsJson @('--save', $shafts)) -and (Test-Path $shafts)) { Write-Host 'ok   the shafts' }

$old = Join-Path $work 'old.mitcad'
$oldJson = @'
[
  {"cmd": "add_feature", "def": {"type": "gear", "kind": "spur", "plane": "xy", "center": [40, 25],
    "module": 2, "teeth": 20, "face_width": 10, "operation": "new_body"}}
]
'@
if ((Invoke-Cli 'old' $oldJson @('--save', $old)) -and (Test-Path $old)) { Write-Host 'ok   the gear of an earlier version' }

# --- The runs

$runs = Join-Path $Out 'runs'

# Invoke-Mitcad name arguments environment: runs the app until it exits;
# its log (Qt messages go to stderr with QT_FORCE_STDERR_LOGGING) and exit
# code.
function Invoke-Mitcad([string]$Name, [string]$Dir, [string[]]$Arguments, [hashtable]$Environment) {
  $info = New-Object System.Diagnostics.ProcessStartInfo $App
  $info.Arguments = ($Arguments | ForEach-Object { '"' + $_ + '"' }) -join ' '
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.RedirectStandardError = $true
  $info.EnvironmentVariables['QT_FORCE_STDERR_LOGGING'] = '1'
  # Where faces, edges and vertices can be clicked ("Pick face ... at x,y").
  $info.EnvironmentVariables['MITCAD_LOG_PICKS'] = '1'
  # Folders of the run's own, not the user's.
  $info.EnvironmentVariables['MITCAD_AUTOSAVE_DIR'] = Join-Path $Dir 'autosave'
  $info.EnvironmentVariables['MITCAD_CRASH_DIR'] = Join-Path $Dir 'crashes'
  $info.EnvironmentVariables['MITCAD_THUMBNAIL_DIR'] = Join-Path $Dir 'thumbnails'
  $info.EnvironmentVariables['MITCAD_RESULT_STORE'] = 'off'
  $info.EnvironmentVariables['MITCAD_NO_UPDATE_CHECK'] = '1'
  foreach ($variable in $Environment.GetEnumerator()) {
    $info.EnvironmentVariables[$variable.Key] = $variable.Value
  }
  $process = [System.Diagnostics.Process]::Start($info)
  $stdout = $process.StandardOutput.ReadToEndAsync()
  $stderr = $process.StandardError.ReadToEndAsync()
  $timedOut = $false
  if (-not $process.WaitForExit(300000)) {
    $process.Kill()
    $timedOut = $true
  }
  $process.WaitForExit()
  $log = $stdout.Result + $stderr.Result
  Set-Content -Encoding utf8 (Join-Path $Dir "$Name.log") $log
  return @{ Log = $log; ExitCode = $process.ExitCode; TimedOut = $timedOut }
}

# Invoke-Shots name chrome design shots parts: one run of the app with a
# copy of the design and the driver script (its parts one after the
# other); which of its shots were saved, and for the others the step that
# failed.
function Invoke-Shots([string]$Name, [string]$Chrome, [string]$Design, [string[]]$Shots, [string[]]$Parts) {
  Write-Host "--- $Name ($($Shots -join ', '))"
  if (-not (Test-Path $Design)) {
    foreach ($shot in $Shots) {
      Write-Host "FAIL: $shot (no $(Split-Path -Leaf $Design): mitcad-cli could not make it)"
      $script:failed.Add("${shot} (${Name}): no $(Split-Path -Leaf $Design)")
    }
    return
  }
  $dir = Join-Path $runs $Name
  Remove-Item -Recurse -Force $dir -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force (Join-Path $dir 'settings\Mitcad') | Out-Null
  New-Item -ItemType Directory -Force (Join-Path $dir 'projects') | Out-Null
  # A copy of the design of the run's own: nothing of one run (a lock, an
  # autosave) reaches the next.
  $copy = Join-Path $dir (Split-Path -Leaf $Design)
  Copy-Item $Design $copy
  $driverScript = Join-Path $dir "$Name.script"
  Set-Content -Encoding ascii $driverScript ($Parts -join "`r`n")
  $result = Invoke-Mitcad $Name $dir @('--no-recovery', '--no-native-dialogs', '--no-mcp', "--chrome=$Chrome", '--open', $copy) @{
    'MITCAD_TEST_INPUT' = $driverScript; 'MITCAD_TEST_COLOR_SCHEME' = 'light'
    'MITCAD_SETTINGS_DIR' = (Join-Path $dir 'settings'); 'MITCAD_PROJECTS_DIR' = (Join-Path $dir 'projects')
    'MITCAD_SHOTS_DIR' = $Out
    'QT_SCALE_FACTOR' = $Scale.ToString([Globalization.CultureInfo]::InvariantCulture) }
  # Why the run stopped: the driver's failed step, with its script line.
  $why = ''
  $fail = $result.Log -split "`r?`n" | Where-Object { $_ -match 'TestDriver: FAIL line ([0-9]+): ' } | Select-Object -Last 1
  if ($fail) {
    $number = [int]([regex]::Match($fail, 'FAIL line ([0-9]+)').Groups[1].Value)
    $lines = Get-Content $driverScript
    $step = if ($number -ge 1 -and $number -le $lines.Count) { $lines[$number - 1].Trim() } else { '' }
    $why = ($fail -replace '^.*TestDriver: ', '') + $(if ($step) { " [$step]" } else { '' })
  } elseif ($result.TimedOut) {
    $why = 'Mitcad did not exit within 300 s'
  } elseif ($result.ExitCode -ne 0) {
    $why = "exit code $($result.ExitCode)"
  }
  foreach ($shot in $Shots) {
    if (Test-Path (Join-Path $Out $shot)) {
      Write-Host "ok   $shot"
      $script:saved.Add($shot)
    } else {
      $reason = if ($why) { $why } else { 'no file, though the run ended without a failed step' }
      Write-Host "FAIL: $shot ($reason); the log: $(Join-Path $dir "$Name.log")"
      $script:failed.Add("${shot} (${Name}): $reason")
    }
  }
}

# Get-StartPart bodies: each script starts once the window shows the light
# theme and the design's bodies, fitted to the view (F6).
function Get-StartPart([string]$Bodies) {
  return @"
expect-ever 'Asking the platform for the light colour scheme' 30
expect-ever 'View area' 60
expect-ever 'Bodies shown: $Bodies' 60
sleep 1000
key F6
wait-idle
sleep 1500
"@
}

# 01-03: Box on the plate's top face, measured from its two nearest edges;
# Distance 1's value box opened, typed; the angle typed in its value box.
$box = @'
mark
command solid.box
expect-log 'Command Box started' 30
expect-log 'Pick face F2\.b0/F2:end\(' 10
mark
click-pick 'face F2\.b0/F2:end\('
expect-log 'Box Place On: 1 face' 10
expect-log 'Place Box: face face F2:end\(' 10
expect-log 'Value box Box at_offset1 at ' 30
expect-log 'Value box Box at_offset2 at ' 30
expect-log 'Preview Box: ok' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\01-box-on-face.png'
mark
click-logged 'Value box Box at_offset1 at '
expect-log 'Value box Box at_offset1 opened: ' 10
sleep 1000
screenshot '${MITCAD_SHOTS_DIR}\02-value-box-open.png'
type 15
key Return
expect-log 'Value box Box at_offset1 closed: 15' 10
mark
type-in 'Panel Box input at_offset2 at ' 12
expect-log 'Preview Box: ok' 30
mark
click-logged 'Panel Box input at_more at '
type-in 'Panel Box input at_angle at ' 10
expect-log 'Value box Box at_angle at ' 30
mark
click-logged 'Value box Box at_angle at '
expect-log 'Value box Box at_angle opened: ' 10
type 30
key Return
expect-log 'Value box Box at_angle closed: 30' 10
expect-log 'Preview Box: ok' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\03-box-angle.png'
'@
Invoke-Shots 'box' 'floating' $plate @('01-box-on-face.png', '02-value-box-open.png', '03-box-angle.png') @(
  (Get-StartPart 'F2\.b0'), $box)

# 04, 05: Pulley on the first shaft's top edge; OK; a second pulley on the
# second shaft with Check Belt the first. Place On takes the picks as the
# panel opens ("Pulley Place On takes now", the pick places logged then);
# a click on its field is not needed. The pulley waits for that pick
# (mitcad#656): no preview before it. Offset is under More. Check Belt is
# right under Place On, in the panel's view without scrolling.
$pulleys = @'
mark
command solid.pulley
expect-log 'Command Pulley started' 30
expect-log 'Pulley Place On takes now: ' 10
expect-log 'Pick edge F1\.b0/E\{F1:side0\|F1:top\} at' 10
mark
click-pick 'edge F1\.b0/E\{F1:side0\|F1:top\} at'
expect-log 'Place Pulley: axis edge E\{F1:side0\|F1:top\} of F1\.b0' 10
expect-log 'bore 8\.2 mm \(shaft 8 mm\)' 30
mark
click-logged 'Panel Pulley input at_more at '
type-in 'Panel Pulley input at_offset at ' 2
expect-log 'Preview Pulley: ok' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\04-pulley-on-shaft.png'
mark
button '^OK$'
expect-log 'Added pulley \(Toothed\), GT2 2 mm, 20 teeth' 30
wait-idle
mark
command solid.pulley
expect-log 'Command Pulley started' 30
expect-log 'Pulley Place On takes now: ' 10
expect-log 'Pick edge F2\.b0/E\{F2:side0\|F2:top\} at' 10
mark
click-pick 'edge F2\.b0/E\{F2:side0\|F2:top\} at'
expect-log 'Place Pulley: axis edge E\{F2:side0\|F2:top\} of F2\.b0' 10
mark
type-in 'Panel Pulley input teeth at ' 28
click-logged 'Panel Pulley input at_more at '
type-in 'Panel Pulley input at_offset at ' 2
expect-log 'Preview Pulley: ok' 30
wait-idle
mark
click-logged 'Panel Pulley input check_belt at '
expect-log 'Pick places:' 10
expect-log 'Pick face F3\.b0/F3:' 10
mark
click-pick 'face F3\.b0/F3:'
expect-log 'Pulley Check Belt: 1 feature' 10
expect-log 'Pulley belt check with F3: Belt length ' 30
expect-log 'Lines Pulley: 1' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\05-pulley-check-belt.png'
'@
Invoke-Shots 'pulleys' 'floating' $shafts @('04-pulley-on-shaft.png', '05-pulley-check-belt.png') @(
  (Get-StartPart 'F1\.b0, F2\.b0'), $pulleys)

# Gear Pair between the shafts' top edges, ratio 2 (its default), the rows
# listed; New Body (Down in the focused Operation, as the tests choose it)
# previewed. Used by 06/07 and by 12.
$gearPair = @'
mark
command solid.gear_pair
expect-log 'Command Gear Pair started' 30
expect-log 'Pick edge F1\.b0/E\{F1:side0\|F1:top\} at' 10
mark
click-pick 'edge F1\.b0/E\{F1:side0\|F1:top\} at'
expect-log 'Gear Pair First Axis: 1 edge' 10
expect-log 'Pick edge F2\.b0/E\{F2:side0\|F2:top\} at' 10
mark
click-pick 'edge F2\.b0/E\{F2:side0\|F2:top\} at'
expect-log 'Gear Pair candidates: ' 30
expect-log 'center distance 60 mm' 10
expect-log 'Panel Gear Pair options candidate: 2 mm, 20:40' 30
expect-log 'Panel Gear Pair input candidate=0 at ' 10
mark
focus-widget input_operation
key Down
focus-main
expect-log 'Gear Pair: Operation = New Body' 10
expect-log 'Preview Gear Pair: ok' 60
wait-idle
sleep 1500
'@
$gearPairRow = @'
screenshot '${MITCAD_SHOTS_DIR}\06-gear-pair-candidates.png'
mark
click-logged 'Panel Gear Pair input candidate=1 at '
expect-log 'Gear Pair: Gears = 1\.25 mm' 10
expect-log 'Preview Gear Pair: ok' 60
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\07-gear-pair-preview.png'
'@
Invoke-Shots 'gear-pair' 'floating' $shafts @('06-gear-pair-candidates.png', '07-gear-pair-preview.png') @(
  (Get-StartPart 'F1\.b0, F2\.b0'), $gearPair, $gearPairRow)

# 08: Belt Drive between the same shafts, ratio 2 and GT2 2 mm (its
# defaults), New Body, the first row previewed.
$beltDrive = @'
mark
command solid.belt_drive
expect-log 'Command Belt Drive started' 30
expect-log 'Pick edge F1\.b0/E\{F1:side0\|F1:top\} at' 10
mark
click-pick 'edge F1\.b0/E\{F1:side0\|F1:top\} at'
expect-log 'Belt Drive First Axis: 1 edge' 10
expect-log 'Pick edge F2\.b0/E\{F2:side0\|F2:top\} at' 10
mark
click-pick 'edge F2\.b0/E\{F2:side0\|F2:top\} at'
expect-log 'Belt Drive candidates: ' 30
expect-log 'Panel Belt Drive options candidate: ' 30
mark
focus-widget input_operation
key Down
focus-main
expect-log 'Belt Drive: Operation = New Body' 10
expect-log 'Preview Belt Drive: ok' 60
expect-log 'Panel Belt Drive note: Belt length ' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\08-belt-drive.png'
'@
Invoke-Shots 'belt-drive' 'floating' $shafts @('08-belt-drive.png') @((Get-StartPart 'F1\.b0, F2\.b0'), $beltDrive)

# 09: Hole on the plate's top face, 20 and 15 from its nearest edges.
$hole = @'
mark
command solid.hole
expect-log 'Command Hole started' 30
expect-log 'Pick face F2\.b0/F2:end\(' 10
mark
click-pick 'face F2\.b0/F2:end\('
expect-log 'Hole Place On: 1 face' 10
expect-log 'Place Hole: face face F2:end\(' 10
expect-log 'Value box Hole at_offset1 at ' 30
mark
type-in 'Panel Hole input at_offset1 at ' 20
type-in 'Panel Hole input at_offset2 at ' 15
type-in 'Panel Hole input diameter at ' 8
expect-log 'Preview Hole: ok' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\09-hole-on-face.png'
'@
Invoke-Shots 'hole' 'floating' $plate @('09-hole-on-face.png') @((Get-StartPart 'F2\.b0'), $hole)

# 10: Cylinder at Sketch2's point (its only point; the sketch's points are
# logged where a click picks them, "Sketch entity F3/p1 at x,y"). Unlike
# the pick places, those places do not avoid the floating panels: the point
# lies in front of the plate, which F6 puts low in the middle of the view,
# clear of the Browser (top left) and the command panel (right, from y
# 358); at (110, 30) it was under the command panel.
$cylinder = @'
mark
command solid.cylinder
expect-log 'Command Cylinder started' 30
expect-log 'Sketch entity F3/p[0-9]+ at' 10
mark
click-logged 'Sketch entity F3/p[0-9]+ at'
expect-log 'Place Cylinder: point ' 10
expect-log 'Preview Cylinder: ok' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\10-cylinder-at-point.png'
'@
Invoke-Shots 'cylinder' 'floating' $plate @('10-cylinder-at-point.png') @((Get-StartPart 'F2\.b0'), $cylinder)

# 11: Gear1, written with its plane and centre, opened for editing.
$oldGear = @'
mark
dblclick-logged '^Timeline Gear1 at '
expect-log 'Editing F1 with Gear' 30
expect-log 'Place Gear: face plane xy' 10
expect-log 'Preview Gear: ok' 30
wait-idle
sleep 1500
screenshot '${MITCAD_SHOTS_DIR}\11-edit-old-design.png'
'@
Invoke-Shots 'edit-old' 'floating' $old @('11-edit-old-design.png') @((Get-StartPart 'F1\.b0'), $oldGear)

# 12: the Gear Pair of 06 in the Docked chrome.
$docked = @'
screenshot '${MITCAD_SHOTS_DIR}\12-docked-gear-pair.png'
'@
Invoke-Shots 'docked-gear-pair' 'docked' $shafts @('12-docked-gear-pair.png') @(
  (Get-StartPart 'F1\.b0, F2\.b0'), $gearPair, $docked)

Write-Host '--- Summary'
Write-Host "Saved ($($saved.Count) of $($shots.Count)):"
foreach ($shot in $saved) { Write-Host "  $(Join-Path $Out $shot)" }
if ($failed.Count -gt 0) {
  Write-Host "Failed ($($failed.Count)):"
  foreach ($shot in $failed) { Write-Host "  $shot" }
}
Write-Host "Screenshots: $Out; logs and driver scripts: $runs"
if ($failed.Count -gt 0) { Write-Host 'FAIL: placement screenshots'; exit 1 }
Write-Host 'PASS: placement screenshots'
