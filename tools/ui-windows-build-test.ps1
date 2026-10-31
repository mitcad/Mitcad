# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli
# Input-driven Windows UI test of the BUILD tab (mitcad#375) in the build
# VM (tools/ui-windows-lib.ps1 says how the input is posted); the Linux
# test is tools/ui-build-test.sh, with the same steps. A library of small
# synthetic BUILD parts made here (mitcad-cli; no supplier's files), fetched
# from a local git repository; skipped where git is not installed:
#   1. The BUILD tab beside SOLID.
#   2. A new design, Component1 active (mitcad#394): Profile adds to the root,
#      no policy message; the first profile by a pick on the XY plane,
#      then (undone) by Enter at the origin; then only profiles' faces,
#      the planes hidden; a side click flush with an end fills Offset and
#      From (no check box); an end click in the middle straight on (Mitre),
#      the hover near an edge previewing nothing (mitcad#508), the click
#      there a square corner on
#      the end face (mitre Auto: none, mitcad#427; Shared chosen mitres it
#      at offset 0; mitcad#417); a brace on its side by Tilt (its foot cut
#      by the tilt, mitcad#444; the Tilt row only on a side); Close shows
#      the planes again.
#   3. Profile on a design with only its root: a frame of four (the mouse
#      on the first's side names only the face a click picks, nothing
#      previewed before the click; once picked, the preview stays while the
#      mouse moves along the side, off the parts and back, no inspection,
#      its end A unchanged, and a click on another face moves it once,
#      mitcad#508; the click's body as long as its length, mitcad#431; two
#      flush with the first's ends by clicks at the flush
#      snaps, From End A and End B; a typed offset stays from end B, From
#      changed keeps the part in place; the fourth's length bound to the
#      third; Tab steps the auto length, the preview's body as long as the
#      length in each mode, mitcad#431).
#   4. Connector at a junction (shown on the hover), previewed in place.
#   5. Panel from an inner face.
#   6. Linear Part: a rail, a carriage on it, a slider on two sides of
#      a profile, each on the side picked (mitcad#448; its hover names
#      the side and previews nothing, mitcad#508) (each chosen in the part
#      picker from the Part
#      tile); Ends: a mitred, bored end.
#   7. Save, reopen; Cut List: rows, Copy as CSV, Save as PDF.
#   8. The part picker (mitcad#418), with a 30 profile of another system in
#      the library's next version: opened from Profile's Part tile
#      (mitcad#427), the groups as a tree (mitcad#527: Slot profile with
#      its Profiles, listing them, folded and unfolded), a group, a click
#      choosing the 30 profile that the
#      preview then uses, a star (first in its group), pinned it follows
#      Connector (a click chooses the bracket); Profile again starts with
#      the 30 profile and opens it filtered to its system, Favourites, In
#      this design after the placement (on a plane; after Place nothing
#      previewed at the origin on top of it, mitcad#513). Its clicks are posted to the
#      picker's own window.
#   9. The part a command starts with (mitcad#452), with a second 30
#      profile of the same system in the library's next version, starred
#      too: a new design starts with the first favourite (the picker's
#      first tile), the design that placed the other one with that one, a
#      new design again with the first favourite, which the picker shows.
#  10. Shift + click places at once (mitcad#451): on the XY plane and on a
#      profile's side without Place; a plain click only previews; over the
#      part's longest length it is refused and places nothing.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-build-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir] [-Shots dir]
# -Shots (default: UI_BUILD_SHOTS, as in the Linux test) saves a screenshot
# of the window there for each length mode of the bound fourth profile.
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-build'),
  [string]$QtBin = '',
  [string]$Shots = $env:UI_BUILD_SHOTS)

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'build.profile' = 'F3'; 'build.panel' = 'F5'; 'build.linear' = 'F7';
                            'build.connector' = 'F8'; 'build.cut_list' = 'F9'; 'build.ends' = 'F11' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows BUILD test (skipped)'
  exit 0
}
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
$library = Join-Path $work 'test-profiles'
$file = Join-Path $work 'frame.mitcad'
$pdf = Join-Path $work 'cut-list.pdf'
# The log gives paths with forward slashes.
$fileLogged = $file -replace '\\', '/'
$pdfLogged = $pdf -replace '\\', '/'

function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}
# Design name script: the commands saved as the library's design <name>.
function Design([string]$Name, [string]$Script) {
  $json = Join-Path $work (($Name -replace '[\\/]', '-') + '.json')
  Set-Content -Encoding ascii $json $Script
  $target = Join-Path $library $Name
  New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
  Run-Cli "run $Name" @('run', $json, '--save', $target)
}

Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-profiles', '--name', 'Test profiles', '--license', 'MIT')
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
function Frame([string]$Name, [string]$Origin, [string]$Z, [string]$X) {
  return ('  {"cmd": "add_feature", "name": "mount:' + $Name + '", "def": {"type": "joint_origin", "geometry": "origin", ' +
          '"frame_override": {"origin": ' + $Origin + ', "z_axis": ' + $Z + ', "x_axis": ' + $X + '}}},')
}
function Bar([double]$Size) {
  $half = $Size / 2
  return @(
    '[',
    '  {"cmd": "add_parameter", "name": "L", "expression": "500 mm"},',
    '  {"cmd": "sketch.create", "plane": "xy"},',
    ('  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-' + $half + ', -' + $half + '], "width": ' + $Size +
     ', "height": ' + $Size + '},'),
    ('  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect + '"}], ' +
     '"extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},'),
    (Frame 'end_a' '[0, 0, 0]' '[0, 0, -1]' '[1, 0, 0]'),
    ('  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin", ' +
     '"geometry": {"body": "F2.b0", "face": "F2:end(' + $rect + ')"}}},')) -join "`n"
}
function Block([string]$X, [string]$Y, [string]$Z) {
  return ('[' + "`n" + '  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"}, ' +
          '"corner": [0, 0], "length": ' + $X + ', "width": ' + $Y + ', "height": ' + $Z + ', "operation": "new_body"}},')
}
$end = "`n" + '  {"expect": {"bodies": 1}}' + "`n" + ']'
$profile20 = @((Bar 20), (Frame 'slot_x+_1' '[10, 0, 0]' '[1, 0, 0]' '[0, 0, 1]'),
  (Frame 'slot_x-_1' '[-10, 0, 0]' '[-1, 0, 0]' '[0, 0, 1]'), (Frame 'slot_y+_1' '[0, 10, 0]' '[0, 1, 0]' '[0, 0, 1]'),
  (Frame 'slot_y-_1' '[0, -10, 0]' '[0, -1, 0]' '[0, 0, 1]')) -join "`n"
Design 'profiles\p20-test.mitcad' ($profile20 + $end)
$bracket = @((Block '18' '18' '18'), (Frame 'mount_a' '[9, 9, 0]' '[0, 0, -1]' '[1, 0, 0]'),
  (Frame 'mount_b' '[0, 9, 9]' '[-1, 0, 0]' '[0, 0, 1]')) -join "`n"
Design 'connectors\bracket-20-test.mitcad' ($bracket + $end)
$panel = @('[',
  '  {"cmd": "add_parameter", "name": "W", "expression": "100 mm"},',
  '  {"cmd": "add_parameter", "name": "H", "expression": "100 mm"},',
  '  {"cmd": "add_parameter", "name": "t", "expression": "3 mm"},',
  # Centred on its XY origin: width along X, height along Y, thickness along
  # Z. A box's corner takes numbers only: the box from the origin (F1),
  # moved back by half its width and height (F2), W and H still sizing it.
  ((Block '"W"' '"H"' '"t"') -split "`n")[1],
  ('  {"cmd": "add_feature", "def": {"type": "move", "bodies": ["F1.b0"], ' +
   '"transform": {"type": "translate_xyz", "x": "-W / 2", "y": "-H / 2", "z": 0}}},'),
  (Frame 'face' '[0, 0, 0]' '[0, 0, -1]' '[1, 0, 0]')) -join "`n"
Design 'panels\panel-test.mitcad' ($panel + $end)
$rail15 = @((Bar 15), (Frame 'rail' '[0, 7.5, 0]' '[0, 1, 0]' '[0, 0, 1]'), (Frame 'base' '[0, -7.5, 0]' '[0, -1, 0]' '[0, 0, 1]')) -join "`n"
Design 'linear\rail-test.mitcad' ($rail15 + $end)
$carriage = @((Block '30' '20' '20'), (Frame 'rail' '[15, 10, 0]' '[0, 0, -1]' '[1, 0, 0]')) -join "`n"
Design 'linear\carriage-test.mitcad' ($carriage + $end)
# A slider that slides in a slot (mitcad#448): its frame on its underside.
$slider = @((Block '20' '10' '5'), (Frame 'slot' '[10, 5, 0]' '[0, 0, -1]' '[1, 0, 0]'),
  (Frame 'top' '[10, 5, 5]' '[0, 0, 1]' '[1, 0, 0]')) -join "`n"
Design 'linear\slider-test.mitcad' ($slider + $end)

Set-Content -Encoding ascii (Join-Path $library 'mitcad-library.json') @'
{
  "format": "mitcad-library", "version": 1, "id": "test-profiles", "name": "Test profiles",
  "description": "Synthetic BUILD parts of the UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "slot", "name": "Slot profile"}, {"id": "profiles", "name": "Profiles", "parent": "slot"},
                 {"id": "connectors", "name": "Connectors", "parent": "slot"},
                 {"id": "panels", "name": "Slot profile/Panels"}, {"id": "linear", "name": "Motion/Linear parts"}],
  "components": [
    {"id": "p20-test", "path": "profiles/p20-test.mitcad", "name": "Profile 20 test", "category": "profiles",
     "supplier": "Test supplier", "part_number": "P20-T", "designation": "Profile 20 test L={L}",
     "build": {"role": "profile", "system": {"type": "I", "slot": 5, "grid": 20},
       "length": {"parameter": "L", "min": "10 mm", "max": "3000 mm", "stock": ["1000 mm"]},
       "section": {"width": "20 mm", "height": "20 mm"},
       "slot": {"depth": "6 mm", "opening": "5.2 mm", "inner_width": "11 mm"}, "mass_per_metre": "0.4 kg",
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
                  {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}},
    {"id": "bracket-20-test", "path": "connectors/bracket-20-test.mitcad", "name": "Bracket 20 test",
     "category": "connectors", "supplier": "Test supplier", "part_number": "B20-T",
     "build": {"role": "connector", "systems": [{"type": "I", "slot": 5, "grid": 20}],
       "frames": [{"name": "mount_a", "kind": "face"}, {"name": "mount_b", "kind": "face"}],
       "connector": {"junctions": ["L", "T"], "space": "inner_corner",
                     "mates": [{"frame": "mount_a", "to": "slot"}, {"frame": "mount_b", "to": "slot"}],
                     "fasteners": [], "machining": []}}},
    {"id": "panel-test", "path": "panels/panel-test.mitcad", "name": "Panel test", "category": "panels",
     "build": {"role": "panel", "frames": [{"name": "face", "kind": "face"}],
               "panel": {"mount": "slot", "thickness": "t", "width": "W", "height": "H", "clearance": "0.5 mm"}}},
    {"id": "rail-test", "path": "linear/rail-test.mitcad", "name": "Rail 15 test", "category": "linear",
     "supplier": "Test supplier", "part_number": "R15-T", "designation": "Rail 15 test L={L}",
     "build": {"role": "linear", "systems": [{"type": "I", "slot": 5, "grid": 20}],
       "length": {"parameter": "L", "min": "20 mm", "max": "2000 mm", "stock": ["1000 mm"]},
       "section": {"width": "15 mm", "height": "15 mm"},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"}, {"name": "rail", "kind": "rail"},
                  {"name": "base", "kind": "face"}],
       "linear": {"kind": "rail"}}},
    {"id": "carriage-test", "path": "linear/carriage-test.mitcad", "name": "Carriage 15 test", "category": "linear",
     "supplier": "Test supplier", "part_number": "C15-T",
     "build": {"role": "linear", "frames": [{"name": "rail", "kind": "rail"}],
       "linear": {"kind": "carriage", "mate": {"frame": "rail", "on": "rail", "joint": "slider", "axis": "x"}}}},
    {"id": "slider-test", "path": "linear/slider-test.mitcad", "name": "Slider 5 test", "category": "linear",
     "supplier": "Test supplier", "part_number": "S5-T",
     "build": {"role": "linear", "systems": [{"type": "I", "slot": 5, "grid": 20}],
       "frames": [{"name": "slot", "kind": "slot"}, {"name": "top", "kind": "face"}],
       "linear": {"kind": "carriage", "mate": {"frame": "slot", "on": "slot", "joint": "slider", "axis": "x"}}}}
  ]
}
'@
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'Test parts'), @('tag', 'v1.0.0'))) {
  $output = & $git.Source -C $library -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
Run-Cli 'library fetch' @('library', 'fetch', $library)
# A design with only its root, the active component.
$empty = Join-Path $work 'empty.json'
Set-Content -Encoding ascii $empty '[]'
$start = Join-Path $work 'start.mitcad'
Run-Cli 'the empty design' @('run', $empty, '--save', $start)

# The occurrence, the name and the body of the part placed last: "Placed <part> as <name> (<occurrence>),
# length ...", and the body the placement logged ("New body <name> (<uid>): volume ...").
function Placed-Occurrence { return Ui-LastMatch '^Placed .* as .* \((O[0-9/]*)\), length' }
function Placed-Name { return Ui-LastMatch '^Placed .* as (.*) \(O[0-9/]*\), length' }
function Placed-Body { return Ui-LastMatch '^New body .* \((F[0-9]+\.b[0-9]+)\): volume' }
# The faces of a body a click can pick now (after the last "Pick places:"),
# whose names contain the text: a derived part's sides are
# "<F>:part(F2:side(c1[c4,c2]))", its ends "<F>:end_a", "<F>:end_b".
function Faces-Of([string]$Body, [string]$Like) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $pattern = '^Pick face ' + [regex]::Escape($Body) + '/(.*) at \d+,\d+$'
  return @($lines[$start..($lines.Count - 1)] | ForEach-Object { if ($_ -match $pattern) { $Matches[1] } } |
    Where-Object { $_.Contains($Like) })
}
function Pick-Face([string]$Body, [string]$Face) { Ui-ClickPick "$Body/$Face at" }
# The view fitted to the parts, and the places a click picks logged again
# (the panel's pick input activated again).
function Refresh-Picks([string]$Command) {
  Ui-Step 'fit the view (F6)'            { Ui-Key 'F6' }
  Ui-Step "$Command's pick again"        { Ui-ClickLogged "Panel $Command input at" }
}
function Place([string]$What) {
  Ui-Mark
  Ui-Step 'Place (Enter)'                { Ui-Key 'Return' }
  Ui-ExpectNew "Placed $What as " "$What placed" 30
}
$side = ':part(F2:side('
# The waits below look at the log every 50 ms (200 ms before, mitcad#870)
# with the same time-outs, the lines since the mark taken as Ui-NewLines
# gives them and a last line as Ui-LastLineWith finds it: a pipeline over a
# long log's every line took long on each look.
#
# Expect-Row INPUT shown|hidden description: Profile's panel row INPUT as
# the panel laid it out last ("Panel Profile input <id> at x,y", with
# " (hidden)" when hidden).
function Expect-Row([string]$Row, [string]$State, [string]$Description) {
  $line = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $line = Ui-LastLineWith "Panel Profile input $Row at "
    $hidden = "$line".EndsWith('(hidden)')
    if ($line -and (($State -eq 'hidden') -eq $hidden)) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Ui-Fail "${Description}: Panel Profile input $Row is not $State ($line)"
}
# Marker-At LABEL: where Profile's last preview marker LABEL (a regex; also
# the taken one, "> LABEL") is: "Preview marker Profile flush B world
# x,y,z at x,y". Its point is on the face picked, so a click there clicks
# that face (mitcad#417).
function Marker-At([string]$Label) {
  $line = Ui-LogLines | Where-Object { $_ -match "^Preview marker Profile (> )?$Label world " } | Select-Object -Last 1
  if ("$line" -notmatch ' at (\d+),(\d+)$') { Ui-Fail "no preview marker '$Label' of Profile logged" }
  return @([int]$Matches[1], [int]$Matches[2])
}
# Marker-InMiddle LABEL: where Profile's marker LABEL is once it lies in
# the middle half of the view, the view zoomed out step by step at its
# centre until then: fitted to the parts, a profile's end can come under
# the orientation cube in the view's corner (session 0's window is small),
# where a click turns the view instead of picking. The zoom rests the mouse
# at the view's centre, which can be on a side (the hover previews nothing
# there, mitcad#508): the mouse goes off the parts (to a corner of the
# view, Pick-Markers) and the pick's markers are read again before each
# look.
function Marker-InMiddle([string]$Label) {
  $area = @((Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)') -split ' ' | ForEach-Object { [int]$_ })
  if ($area.Count -lt 4) { Ui-Fail 'the app did not log its view area' }
  $cx = [int]($area[0] + $area[2] / 2)
  $cy = [int]($area[1] + $area[3] / 2)
  $sign = -1
  $last = $null
  for ($i = 0; $i -lt 16; $i++) {
    Pick-Markers $area
    $at = Marker-At $Label
    $off = [Math]::Max([Math]::Abs($at[0] - $cx) / $area[2], [Math]::Abs($at[1] - $cy) / $area[3])
    if ($off -lt 0.25) { return $at }
    # A step that moved it away zoomed in: the other way then.
    if ($null -ne $last -and $off -gt $last) { $sign = -$sign }
    $last = $off
    [UiInput]::Move($script:UiWindow, $cx, $cy, 0x04)
    Start-Sleep -Milliseconds 80
    [UiInput]::Wheel($script:UiWindow, $cx, $cy, $sign)
    # The zoomed view drawn, its markers logged and the camera at rest
    # (the view's rest timer is watched; mitcad#870: a fixed 500 ms more
    # before).
    Ui-Sync
  }
  Ui-Fail "Profile's marker '$Label' did not come into the middle of the view"
}
# Expect-Inspect TEXTS description: an "Inspect Profile:" line since the
# mark has every text.
function Expect-Inspect([string[]]$Texts, [string]$Description) {
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    foreach ($line in @(Ui-NewLines)) {
      if (-not $line.StartsWith('Inspect Profile: ')) { continue }
      if (@($Texts | Where-Object { -not $line.Contains($_) }).Count -eq 0) { Write-Host "ok   $Description"; return }
    }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 6000)
  Ui-Fail "${Description}: no 'Inspect Profile:' line since the mark with $($Texts -join ' / ')"
}
# Expect-BodyLength description: the last "Inspect Profile:" line since the
# mark that logs the preview body's length ("; body <n> mm", mitcad#431)
# has it equal to the length shown ("length <n> mm").
function Expect-BodyLength([string]$Description) {
  $line = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while (-not $line -and $clock.ElapsedMilliseconds -lt 6000) {
    $line = Ui-NewLines | Where-Object { $_.StartsWith('Inspect Profile: ') -and $_.Contains('; body ') } |
      Select-Object -Last 1
    if (-not $line) { Start-Sleep -Milliseconds 50 }
  }
  if (-not $line) { Ui-Fail "${Description}: no 'Inspect Profile:' line with the body's length since the mark" }
  $length = if ($line -match '^Inspect Profile: place [a-z]*, length ([0-9.]*) mm') { $Matches[1] } else { '?' }
  $body = if ($line -match '; body ([0-9.]*) mm') { $Matches[1] } else { '?' }
  if ($length -eq '?' -or $length -ne $body) { Ui-Fail "${Description}: length $length mm, body $body mm ($line)" }
  Write-Host "ok   ${Description}: $body mm"
}
# No-Inspect CMD description: a moment after the mouse moved, no "Inspect
# CMD:" line since the mark: the hover runs no placement and no inspection
# (mitcad#508).
function No-Inspect([string]$Cmd, [string]$Description) {
  Start-Sleep -Milliseconds 1500
  Ui-Sync
  $found = @(Ui-NewLines | Where-Object { $_.StartsWith("Inspect ${Cmd}: ") })
  if ($found.Count -gt 0) { Ui-Fail "${Description}: $($found[0])" }
  Write-Host "ok   $Description"
}
# Last-EndA CMD: where the last "Inspect CMD:" line puts the preview's end
# A ("; end A at x, y, z").
function Last-EndA([string]$Cmd) {
  $line = Ui-LogLines | Where-Object { $_.StartsWith("Inspect ${Cmd}: ") } | Select-Object -Last 1
  if ("$line" -match '; end A at ([^;]*);') { return $Matches[1] }
  return $null
}
# EndA-Since CMD: the places of the preview's end A in the "Inspect CMD:"
# lines since the mark, each once.
function EndA-Since([string]$Cmd) {
  return @(Ui-NewLines | Where-Object { $_.StartsWith("Inspect ${Cmd}: ") } |
    ForEach-Object { if ($_ -match '; end A at ([^;]*);') { $Matches[1] } } | Select-Object -Unique)
}
# Moved-Once CMD WAS description: after a click, the "Inspect CMD:" lines
# since the mark put the preview's end A elsewhere than WAS, all at one
# place: the preview moved once, to the click (mitcad#508).
function Moved-Once([string]$Cmd, [string]$Was, [string]$Description) {
  $places = @()
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ($places.Count -eq 0 -and $clock.ElapsedMilliseconds -lt 6000) {
    $places = @(EndA-Since $Cmd)
    if ($places.Count -eq 0) { Start-Sleep -Milliseconds 50 }
  }
  # A moment more: a preview still following the mouse would add places.
  Start-Sleep -Milliseconds 1000
  Ui-Sync
  $places = @(EndA-Since $Cmd)
  if ($places.Count -eq 0) { Ui-Fail "${Description}: no 'Inspect ${Cmd}:' line with end A since the mark" }
  if ($places.Count -gt 1) { Ui-Fail "${Description}: end A at more than one place: $($places -join ' / ')" }
  if ($places[0] -eq $Was) { Ui-Fail "${Description}: end A still at $Was" }
  Write-Host "ok   ${Description}: end A at $($places[0])"
}
# Off-Parts AREA: the mouse off the parts. The Linux test moves it out of
# the view; in session 0 the view gets its Leave right after each posted
# move, before the frame that finds the hover, and none when the mouse
# then goes elsewhere: the mouse goes to the view's top left corner
# instead (AREA: x y width height of the view; not the top right one,
# under the orientation cube), where nothing is found.
function Off-Parts([int[]]$Area) {
  [UiInput]::Move($script:UiWindow, ($Area[0] + 6), ($Area[1] + 6), 0x04)
}
function View-Area {
  $area = @((Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)') -split ' ' | ForEach-Object { [int]$_ })
  if ($area.Count -lt 4) { Ui-Fail 'the app did not log its view area' }
  return , $area
}
# Pick-Markers AREA: the mouse moved off the parts (Off-Parts) and the
# pick's markers logged (the hover previews nothing, mitcad#508: they are
# the pick's wherever the mouse is).
function Pick-Markers([int[]]$Area) {
  Off-Parts $Area
  Ui-Sync
  $lines = @(Ui-LogLines)
  $inspect = 0
  for ($j = $lines.Count - 1; $j -ge 0; $j--) { if ($lines[$j].StartsWith('Inspect Profile: ')) { $inspect = $j; break } }
  # The markers come after the inspection's line.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $lines = @(Ui-LogLines)
    for ($j = $inspect; $j -lt $lines.Count; $j++) { if ($lines[$j].StartsWith('Preview marker Profile ')) { return } }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 2500)
}
# Shot NAME: with -Shots, a screenshot of the window there as NAME.png. The
# app runs a test driver script for it (MITCAD_TEST_INPUT,
# app/framework/TestDriver.hpp; Start-Shots) that takes one on each F12,
# which no command has: the posted input of this test cannot capture a
# window in session 0.
$shotNames = @('431-bound-face', '431-bound-centre', '431-bound-through')
function Start-Shots {
  if (-not $Shots) { return }
  New-Item -ItemType Directory -Force $Shots | Out-Null
  $script:Shots = (Resolve-Path $Shots).Path
  $steps = foreach ($name in $shotNames) {
    "wait-log '^Key press: F12' 3600"
    "screenshot '$(Join-Path $script:Shots "$name.png")'"
    'mark'
  }
  $driver = Join-Path $script:UiOut 'shots.txt'
  Set-Content -Encoding ascii $driver (@($steps) + 'stay')
  foreach ($name in $shotNames) { Remove-Item -Force (Join-Path $script:Shots "$name.png") -ErrorAction SilentlyContinue }
  $env:MITCAD_TEST_INPUT = $driver
}
function Shot([string]$Name) {
  if (-not $Shots) { return }
  $path = Join-Path $script:Shots "$Name.png"
  Ui-Step "screenshot $Name (F12)"       { Ui-Key 'F12' }
  for ($i = 0; $i -lt 100 -and -not (Test-Path $path); $i++) { Start-Sleep -Milliseconds 200 }
  if (-not (Test-Path $path)) { Ui-Fail "no screenshot $path" }
  Start-Sleep -Milliseconds 500
  Write-Host "note: screenshot $path"
}
# Wheel-Choice INPUT STEPS: a panel drop-down of Profile moved by STEPS
# entries (positive: down the list), one wheel step at a time.
function Wheel-Choice([string]$Choice, [int]$Steps) {
  for ($i = 0; $i -lt [Math]::Abs($Steps); $i++) {
    Ui-WheelLogged "Panel Profile input $Choice" ([Math]::Sign(-$Steps))
    # The step handled and the panel's places logged again (mitcad#870: a
    # fixed 150 ms before).
    Ui-Sync
  }
}
# No-PolicyMessage: no design-policy refusal reached the log.
function No-PolicyMessage {
  $refused = Ui-LogLines | Where-Object { $_.Contains('does not allow') } | Select-Object -First 1
  if ($refused) { Ui-Fail "a policy refusal: $refused" }
  Write-Host 'ok   no policy message'
}

# Picker-Click TEXT: a click on a place of the part picker that it logged
# ("Part picker tile <part> at x,y; in the picker x,y"), posted to the
# picker's own window at its own coordinates (a window of its own, which
# the main window's clicks do not reach).
function Picker-Click([string]$Text) {
  $place = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while (-not $place -and $clock.ElapsedMilliseconds -lt 6000) {
    $line = Ui-LastLineWith "$Text at "
    $match = [regex]::Match("$line", 'in the picker (-?\d+),(-?\d+)$')
    if ($match.Success) { $place = @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value) }
    else { Start-Sleep -Milliseconds 50 }
  }
  if (-not $place) { Ui-Fail "no place of the part picker logged for '$Text'" }
  # Ui-Windows gives the array itself (as one object): no @( ) around it.
  $picker = Ui-Windows '^Part Picker'
  if ($picker.Count -eq 0) { Ui-Fail 'no part picker window' }
  $b = $script:UiButtons[1]
  # Two clicks on one place (a group's fold, then unfold) not a double-click.
  Ui-Apart $picker[0] $place[0] $place[1]
  [UiInput]::Move($picker[0], $place[0], $place[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($picker[0], $b[0], $place[0], $place[1], $b[3])
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($picker[0], $b[1], $place[0], $place[1], 0)
}

# Choose-InPicker COMMAND PART: the panel's Part tile opens the part picker
# (mitcad#427) and a click on the part's tile there chooses it; the picker
# stays open beside the view until the command ends.
function Choose-InPicker([string]$Command, [string]$Part) {
  Ui-Mark
  Ui-Step 'the Part tile'                { Ui-ClickLogged "Panel $Command input part" }
  Ui-ExpectNew "${Command}: Part button" 'the Part tile opens the part picker'
  Ui-ExpectNew "Part picker tile $Part at " "${Part}'s tile in the picker"
  Ui-Mark
  Ui-Step "$Part in the picker"          { Picker-Click "Part picker tile $Part" }
  Ui-ExpectNew "Part picker chose $Part" "$Part chosen in the picker"
}

# A new design (mitcad#394): Component1 active, the root with the Assembly policy.
Ui-StartApp 'build-new'

Write-Host '--- 1. The BUILD tab'
Ui-ExpectLog 'Ribbon tab BUILD at' 'the BUILD tab beside SOLID'
Ui-Mark
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-ExpectNew 'Ribbon BUILD/PROFILE at' 'its PROFILE group'

Write-Host '--- 2. The first profiles of a new design, Component1 active'
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'Command Profile started' 'Profile started'
Ui-ExpectNew 'BUILD profile adds to Component1 (a ground placement)' 'BUILD adds to the active Component1 (Hybrid, mitcad#421)'
Ui-ExpectNew 'View hint Profile: Click a plane or a face, or press Enter' "the view's hint for the first part"
Ui-ExpectNew 'Inspect Profile: place grounded, length 1000.00 mm (typed), square ends; adds to Component1' 'at the origin with its stock length'
Expect-Row 'axis=z' 'shown' "the Direction's X, Y and Z buttons"
Ui-ExpectNew 'Panel Profile tile part: Profile 20 test' 'the Part tile shows the chosen profile (mitcad#427)'
Expect-Row 'search' 'hidden' 'no Search row: the part picker has it'
Expect-Row 'system' 'hidden' 'no System row: the part picker has it'
Expect-Row 'mitre' 'hidden' 'no Mitre before an end pick'
Expect-Row 'offset' 'hidden' 'no Offset before a side pick'
Expect-Row 'place' 'hidden' 'no Placement before a part to go on (mitcad#427)'
Expect-Row 'tilt' 'hidden' 'no Tilt before a side pick (mitcad#444)'
Ui-Mark
Ui-Step "on the root's XY plane"         { Ui-ClickLogged 'Datum xy' }
Ui-ExpectNew 'Inspect Profile: place grounded, length 1000.00 mm (typed), square ends; adds to Component1' 'on the root''s XY plane, picked from Component1: grounded at the click'
Place 'Profile 20 test'
No-PolicyMessage
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Step 'undo it (Ctrl+Z)'               { Ui-Key 'ctrl+z' }
Ui-Mark
Ui-Step 'Profile again (F3)'             { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD profile adds to Component1 (a ground placement)' 'a new design again'
Ui-Step '60 mm long'                     { Ui-TypeIn 'Panel Profile input length' '60' }
Ui-Mark
Ui-Step 'along X'                        { Ui-ClickLogged 'Panel Profile input axis=x' }
Expect-Inspect @('; along x', '; end A at 0.0, 0.0, 10.0') 'the preview along X, resting on the XY plane (mitcad#492)'
Ui-Mark
Ui-Step 'standing (Z)'                   { Ui-ClickLogged 'Panel Profile input axis=z' }
Expect-Inspect @('; along z', '; end A at 0.0, 0.0, 0.0') 'the preview standing on its end at the origin'
Place 'Profile 20 test'
No-PolicyMessage
$b0 = Placed-Body
if (-not $b0) { Ui-Fail 'no body of the first profile' }
Ui-ExpectNew 'Profile Place On takes now: faces' "only a profile's faces once one is there"
Ui-ExpectNew 'Construction geometry hidden for the command' 'the planes hidden for side and end placements'
Expect-Row 'place=auto' 'shown' "Placement's icon buttons once a part is there (mitcad#427)"
Expect-Row 'length_mode=face' 'shown' "the length mode's icon buttons (mitcad#427)"
Refresh-Picks 'Profile'
$sides0 = @(Faces-Of $b0 $side)
if ($sides0.Count -eq 0) { Ui-Fail "no side of the first profile ($b0) to pick" }
Ui-Mark
Ui-Step 'its side, 60 mm long'           { Pick-Face $b0 $sides0[0] }
Ui-ExpectNew 'Inspect Profile: place side' 'on the side'
Ui-ExpectNew '; snap end_' 'flush with the end near the click'
Ui-ExpectNew 'BUILD side click: From End ' 'the click fills From with the end it is flush with'
Expect-Row 'offset' 'shown' 'Offset for a side pick (no check box)'
Expect-Row 'from' 'shown' 'From for a side pick'
Expect-Row 'mitre' 'hidden' 'no Mitre for a side pick'
Expect-Row 'tilt' 'shown' 'Tilt for a side pick (mitcad#444)'
Place 'Profile 20 test'
$b1 = Placed-Body
Refresh-Picks 'Profile'
# An end of the first profile, or of the second where the first's end is
# out of sight in session 0's smaller window: the step is about an end pick.
$endBody = $b0
$tops0 = @(Faces-Of $b0 ':end_b')
if ($tops0.Count -eq 0 -and $b1) { $endBody = $b1; $tops0 = @(Faces-Of $b1 ':end_b') + @(Faces-Of $b1 ':end_a') }
if ($tops0.Count -eq 0) { Ui-Fail "no end face of the first two profiles ($b0, $b1) to pick" }
# The end face's pick place is in its inner half: straight on (mitcad#417).
Ui-Mark
Ui-Step 'an end face, in the middle'     { Pick-Face $endBody $tops0[0] }
Ui-ExpectNew 'Inspect Profile: place end' 'on the end'
Ui-ExpectNew 'BUILD end click: straight towards ' 'a click in the middle of the end continues straight'
Expect-Inspect @('; end straight towards ', '; least snap ') 'the preview straight on, with the snap distance'
Expect-Row 'direction' 'shown' 'Direction for an end pick'
Expect-Row 'mitre' 'shown' 'Mitre for an end pick'
Expect-Row 'offset' 'hidden' 'no side Offset for an end pick'
Expect-Row 'corner_offset' 'hidden' 'no corner Offset straight on'
Expect-Row 'tilt' 'hidden' 'no Tilt for an end pick (mitcad#444)'
Ui-Mark
Ui-Step 'mitre the new part'             { Wheel-Choice 'mitre' 3 }
Ui-ExpectNew 'ends A 45 deg' "the new part's end mitred"
Ui-Mark
Ui-Step 'the mitre Auto again'           { Wheel-Choice 'mitre' -3 }
Expect-Inspect @('; end straight towards ', 'mitre none') 'Auto: no mitre straight on'
# Near an edge of the end face (its zone's marker): the hover previews
# nothing, the part stays straight on (mitcad#508); the click there turns
# it: a corner.
$turn = Marker-At 'turn .*'
Ui-Sync
Ui-Mark
# MK_SHIFT keeps the move in session 0 (see the Connector's hover below).
Ui-Step 'the mouse near an edge of the end' { [UiInput]::Move($script:UiWindow, $turn[0], $turn[1], 0x04) }
No-Inspect 'Profile' 'the hover near the edge previews and inspects nothing'
Ui-Mark
Ui-Step 'a click there'                  { Ui-Click $turn[0] $turn[1] }
Ui-ExpectNew 'BUILD end click: edge towards ' 'a click near an edge turns the part'
Expect-Inspect @('; end edge towards ', 'mitre none, offset 0.0 mm', 'square ends') 'a square corner on the end face, flush with the outer side: its mitre Auto: none (mitcad#427)'
Expect-Row 'corner_offset' 'shown' "the corner's Offset"
Ui-Mark
Ui-Step "the corner's offset 10"         { Ui-TypeIn 'Panel Profile input corner_offset' '10' }
Expect-Inspect @('; end edge towards ', 'mitre none, offset 10.0 mm') 'at another offset the mitre is none'
Ui-Mark
Ui-Step "the corner's offset 0"          { Ui-TypeIn 'Panel Profile input corner_offset' '0' }
Expect-Inspect @('; end edge towards ', 'mitre none, offset 0.0 mm', 'square ends') 'on the end face again at offset 0'
Ui-Mark
Ui-Step 'Mitre: Shared'                  { Wheel-Choice 'mitre' 2 }
Expect-Inspect @('; end edge towards ', 'mitre shared, offset 0.0 mm', 'ends A 45 deg') 'Shared chosen: both on the bisector'
Place 'Profile 20 test'
No-PolicyMessage
# A brace on the first one's side (mitcad#444): Tilt, its foot cut by the
# tilt to lie on the side.
Refresh-Picks 'Profile'
$sides0 = @(Faces-Of $b0 $side)
if ($sides0.Count -eq 0) { Ui-Fail "no side of the first profile ($b0) to pick for the brace" }
Ui-Mark
Ui-Step "the first profile's side again" { Pick-Face $b0 $sides0[0] }
Ui-ExpectNew 'Inspect Profile: place side' 'on the side'
Expect-Row 'tilt' 'shown' 'Tilt for a side pick again'
Ui-Mark
Ui-Step 'tilt 30 deg'                    { Ui-TypeIn 'Panel Profile input tilt' '30 deg' }
Expect-Inspect @('Inspect Profile: place side, ', 'ends A 30 deg', '; tilt 30.00 deg') 'the preview tilted 30 deg, its foot cut by 30 deg'
Place 'Profile 20 test'
No-PolicyMessage
Ui-Mark
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Closed Profile after 4' 'Close keeps them'
Ui-ExpectNew 'Construction geometry shown again' 'the planes shown again when the command ends'

# The rest on a design with only its root, the active component.
Start-Shots
Ui-StartApp 'build' @('--open', $start)
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }

Write-Host '--- 3. Profile: a frame of four'
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'Command Profile started' 'Profile started'
Ui-ExpectNew 'BUILD parts profile: 1 (0 in the design)' 'the picker lists the profile'
Ui-Step 'a typed length of 400'          { Ui-TypeIn 'Panel Profile input length' '400' }
Ui-ExpectNew 'BUILD length typed: detached from the auto length' 'typing a length detaches it'
Ui-Step 'on the XY plane'                { Ui-ClickLogged 'Datum xy' }
Ui-ExpectNew 'Inspect Profile: place ' 'the preview'
Place 'Profile 20 test'
Ui-ExpectNew 'Applied Profile (1)' 'the panel stays open'
$p1 = Placed-Occurrence
$b1 = Placed-Body
if (-not $p1 -or -not $b1) { Ui-Fail 'no occurrence or body of the first profile' }
Refresh-Picks 'Profile'
# The two on a side of the first one that a click can reach, flush with
# its ends by clicks at the flush snaps (their markers): a click fills
# Offset and From (mitcad#417).
$sides1 = @(Faces-Of $b1 $side)
if ($sides1.Count -eq 0) { Ui-Fail "no side of the first profile ($b1) to pick" }
$side1 = $sides1[0]
# The mouse on the side before a click names only the face a click picks:
# nothing previewed or inspected while it moves (mitcad#508).
$hover = Ui-PickAt "$b1/$side1 at"
Ui-Sync
Ui-Mark
Ui-Step "the mouse on the first profile's side" { [UiInput]::Move($script:UiWindow, $hover[0], $hover[1], 0x04) }
Ui-ExpectNew "BUILD hover pick: face $side1 of $b1" 'the hover names the face a click picks'
Ui-Step 'the mouse a little further'     { [UiInput]::Move($script:UiWindow, ($hover[0] + 6), $hover[1], 0x04) }
No-Inspect 'Profile' 'no preview and no inspection while hovering before the click'
# The click's body as long as the length it shows (mitcad#431).
Ui-Mark
Ui-Step "the first profile's side"       { Pick-Face $b1 $side1 }
Ui-ExpectNew 'Inspect Profile: place side' 'on the side'
Ui-ExpectNew 'BUILD side click: From ' 'the click fills From and Offset'
Expect-BodyLength "the click's body as long as its length"
# Once the place is picked the preview stays there (mitcad#508): the mouse
# along the side, off the parts and back runs no placement and no
# inspection, the preview's end A where the click put it; a click on
# another face moves it there once, a click on the first place back again.
Ui-Sync
$endA1 = Last-EndA 'Profile'
if (-not $endA1) { Ui-Fail "no end A in the side pick's inspection" }
Ui-Mark
Ui-Step 'the mouse along the side'       { [UiInput]::Move($script:UiWindow, ($hover[0] + 12), $hover[1], 0x04) }
Ui-Step 'the mouse off the parts'        { Off-Parts (View-Area) }
Ui-ExpectNew "BUILD hover pick: off the parts' faces" 'the hover off the parts'
Ui-Step 'the mouse on the side again'    { [UiInput]::Move($script:UiWindow, $hover[0], $hover[1], 0x04) }
No-Inspect 'Profile' 'no preview and no inspection while the mouse moves after the click'
if ((Last-EndA 'Profile') -ne $endA1) { Ui-Fail "the preview's end A moved from $endA1" }
Write-Host "ok   the preview's end A still at $endA1"
$other1 = @($sides1 | Where-Object { $_ -ne $side1 }) + @(Faces-Of $b1 ':end_b') | Select-Object -First 1
if (-not $other1) { Ui-Fail "no other face of the first profile ($b1) to click" }
Ui-Mark
Ui-Step 'a click on another face'        { Pick-Face $b1 $other1 }
Moved-Once 'Profile' $endA1 "a click on $other1 moves the preview once"
Ui-Mark
Ui-Step "the first profile's side again" { Pick-Face $b1 $side1 }
Ui-ExpectNew 'BUILD side click: From ' 'the click on the side again'
Ui-Sync
if ((Last-EndA 'Profile') -ne $endA1) { Ui-Fail "the click on the side again: end A not at $endA1" }
Write-Host "ok   the click on the side again: end A at $endA1"
Expect-Row 'offset' 'shown' 'Offset for a side pick (no check box)'
Expect-Row 'from' 'shown' 'From for a side pick'
Ui-Step 'length 300'                     { Ui-TypeIn 'Panel Profile input length' '300' }
$flushA = Marker-InMiddle 'flush A'
Ui-Mark
Ui-Step 'a click flush with end A'       { Ui-Click $flushA[0] $flushA[1] }
Ui-ExpectNew 'BUILD side click: From End A, Offset 0 mm (end_a)' 'flush with end A: From End A, Offset 0'
Expect-Inspect @('; snap end_a at 0.0 mm from a') 'the preview flush with end A'
Place 'Profile 20 test'
$p2 = Placed-Occurrence
$b2 = Placed-Body
Refresh-Picks 'Profile'
# Flush with end B by a click, then typed offsets stay from end B; From
# changed keeps the part where it is (L 400, width 20).
Ui-Mark
Ui-Step "the first profile's side again" { Pick-Face $b1 $side1 }
Ui-ExpectNew 'Inspect Profile: place side' 'on the side'
$flushB = Marker-InMiddle 'flush B'
Ui-Mark
Ui-Step 'a click flush with end B'       { Ui-Click $flushB[0] $flushB[1] }
Ui-ExpectNew 'BUILD side click: From End B, Offset 0 mm (end_b)' 'flush with end B: From End B, Offset 0'
Expect-Inspect @('; snap end_b at 0.0 mm from b') 'the preview flush with end B'
Ui-Mark
Ui-Step 'offset 30'                      { Ui-TypeIn 'Panel Profile input offset' '30' }
Ui-ExpectNew 'BUILD side offset typed: from end B' 'a typed offset keeps From'
Expect-Inspect @(' at 30.0 mm from b') '30 mm from end B'
Ui-Mark
Ui-Step 'from end A'                     { Wheel-Choice 'from' -1 }
Ui-ExpectNew 'BUILD From End A keeps the part in place: Offset 350 mm' 'From End A keeps the part in place'
Expect-Inspect @(' at 350.0 mm from a') '350 mm from end A: the same place'
Ui-Mark
Ui-Step 'from the middle'                { Wheel-Choice 'from' 2 }
Ui-ExpectNew 'BUILD From Middle keeps the part in place: Offset 160 mm' 'From Middle keeps the part in place'
Expect-Inspect @(' at 160.0 mm from middle') '160 mm from the middle: the same place'
Ui-Mark
Ui-Step 'from end B again'               { Wheel-Choice 'from' -1 }
Ui-ExpectNew 'BUILD From End B keeps the part in place: Offset 30 mm' 'From End B again'
Ui-Mark
Ui-Step 'offset 0'                       { Ui-TypeIn 'Panel Profile input offset' '0' }
Expect-Inspect @(' at 0.0 mm from b') 'offset 0 from end B: flush with end B'
Place 'Profile 20 test'
$p3 = Placed-Occurrence
$b3 = Placed-Body
$p3Name = Placed-Name
Write-Host "ok   the profiles are $p1 ($b1), $p2 ($b2), $p3 ($b3, $p3Name), on $side1"
Ui-Mark
Ui-Step 'the auto length again (Tab)'    { Ui-Key 'Tab' }
Ui-ExpectNew 'Profile: Tab steps Length' 'Tab steps the auto length'
Refresh-Picks 'Profile'
$found = $null
foreach ($face in @(Faces-Of $b2 $side)) {
  Ui-Mark
  Ui-Step "the second profile's $face"   { Pick-Face $b2 $face }
  $inspect = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $inspect = Ui-NewLines | Where-Object { $_.StartsWith('Inspect Profile: ') } | Select-Object -Last 1
    if ($inspect) { break }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 5000)
  # Bound to the third: "(bound, auto: to the face to <its face> of <name>)" or "... to <name>)".
  if ("$inspect".Contains("(bound, ") -and "$inspect".Contains(" $p3Name)")) { $found = $face; break }
}
if (-not $found) { Ui-Fail 'no side of the second profile a click reaches has its auto length to the third' }
Write-Host "ok   $found of the second profile runs to the third"
# The preview's body ends at the third profile, in each mode (mitcad#431).
Expect-BodyLength "the body to the third profile's face"
Shot '431-bound-face'
Ui-Mark
Ui-Step 'to the centre (Tab)'            { Ui-Key 'Tab' }
Ui-ExpectNew 'Profile: Length = Auto: to its centre' 'Tab: to the centre'
Ui-ExpectNew '(bound, auto: to the centre to ' "bound to the third profile's centre"
Expect-BodyLength "the body to the third profile's centre"
Shot '431-bound-centre'
Ui-Mark
Ui-Step 'through (Tab)'                  { Ui-Key 'Tab' }
Ui-ExpectNew 'Profile: Length = Auto: through it' 'Tab: through'
Ui-ExpectNew '(bound, auto: through to ' 'bound through the third profile'
Expect-BodyLength 'the body through the third profile'
Shot '431-bound-through'
Ui-Step 'to the face (Tab)'              { Ui-Key 'Tab' }
Ui-ExpectNew 'Profile: Length = Auto: to the face it meets' 'Tab: to the face again'
Ui-ExpectNew 'Cursor box Profile length: ' 'the auto length in the box at the cursor'
Ui-ExpectNew '(bound, auto: to the face to ' "bound to the third profile's face"
Place 'Profile 20 test'
$b4 = Placed-Body
Ui-ExpectNew 'length bound, auto: to the face' 'its length is bound'
Ui-Mark
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Closed Profile after 4' 'Close keeps the four profiles'

Write-Host '--- 4. Connector'
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'Connector (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Inspect Connector: junction ' 'the junction and its connectors'
Ui-ExpectNew ', preview Bracket 20 test with ' 'the chosen connector previewed in place'
Ui-ExpectNew 'Panel Connector tile part: Bracket 20 test' 'the chosen connector shown as a picture tile (mitcad#427)'
$faces3 = @(Faces-Of $b3 $side)
if ($faces3.Count -eq 0) { Ui-Fail "no side of the third profile ($b3) to hover" }
Ui-Mark
$over = Ui-PickAt "$b3/$($faces3[0]) at"
# A move with nothing in wParam is taken by Qt for the "enter" of a window
# shown under the cursor and dropped while no window is under the pointer,
# which in session 0 (no cursor) is the case after the panels changed:
# MK_SHIFT in wParam keeps the move (Qt reads the modifiers from the key
# state, so it is a plain move over the view).
Ui-Step 'the mouse over the third profile' { [UiInput]::Move($script:UiWindow, $over[0], $over[1], 0x04) }
$shown = $false
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $shown -and $clock.ElapsedMilliseconds -lt 5000) {
  $shown = [bool](Ui-NewLines | Where-Object { $_ -match '^Preview marker Connector [^>]' })
  if (-not $shown) { Start-Sleep -Milliseconds 50 }
}
if (-not $shown) { Ui-Fail 'no junction of the third profile shown on its hover' }
Write-Host "ok   the hovered profile's junctions shown"
Ui-ExpectNew 'Preview marker Connector > ' 'the junctions marked'
Ui-Mark
Ui-Step 'Connect (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Connected Bracket 20 test at junction ' 'the connector placed' 30
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 5. Panel'
Ui-Mark
Ui-Step 'Panel (F5)'                     { Ui-Key 'F5' }
Ui-ExpectNew 'BUILD parts panel: 1' 'the picker lists the panel'
Ui-Step "the frame's inner face"         { Pick-Face $b1 $side1 }
Ui-ExpectNew 'Inspect Panel: panel Panel test' "the panel's preview"
Ui-Mark
Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
Ui-ExpectNew 'Placed panel Panel test as ' 'the panel placed' 30
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 6. Linear Part: a rail, a carriage and a slider'
Ui-Mark
Ui-Step 'Linear Part (F7)'               { Ui-Key 'F7' }
Ui-ExpectNew 'BUILD parts linear: 3' 'the rail, the carriage and the slider'
Choose-InPicker 'Linear Part' 'test-profiles/rail-test'
Ui-Step 'a typed length of 200'          { Ui-TypeIn 'Panel Linear Part input length' '200' }
# Another side of the first profile than the frame's, else that one.
$outer = @(Faces-Of $b1 $side | Where-Object { $_ -ne $side1 })
$railFace = if ($outer.Count -gt 0) { $outer[0] } else { $side1 }
Ui-Step "the first profile's $railFace"  { Pick-Face $b1 $railFace }
Ui-ExpectNew 'Inspect Linear Part: place ' "the rail's preview"
Place 'Rail 15 test'
$bRail = Placed-Body
Choose-InPicker 'Linear Part' 'test-profiles/carriage-test'
Refresh-Picks 'Linear Part'
$railFaces = @(Faces-Of $bRail $side)
if ($railFaces.Count -eq 0) { Ui-Fail "no side of the rail ($bRail) to pick" }
$top = @($railFaces | Where-Object { $_.Contains('side(c3[') })
$railTop = if ($top.Count -gt 0) { $top[0] } else { $railFaces[0] }
Ui-Mark
Ui-Step "the rail's $railTop"            { Pick-Face $bRail $railTop }
Ui-ExpectNew 'Inspect Linear Part: ' "the carriage's preview"
Place 'Carriage 15 test'
# A slider on two sides of a profile (mitcad#448): each on the side picked,
# as a profile on a side, not on the profile's first frame.
Choose-InPicker 'Linear Part' 'test-profiles/slider-test'
Refresh-Picks 'Linear Part'
# Two sides of a profile that a click can reach: the first profile's sides
# are mostly covered (the panel, the rail and the carriage), so of the
# profiles with two side pick places, the rail's last.
$bSlider = $null
$sliderFaces = @()
foreach ($body in @($b2, $b3, $b4, $b1)) {
  if (-not $body) { continue }
  $faces = @(Faces-Of $body $side)
  if ($faces.Count -ge 2) { $bSlider = $body; $sliderFaces = $faces; break }
}
if (-not $bSlider) { Ui-Fail "no profile ($b1, $b2, $b3, $b4) with two sides to pick" }
$slide1 = $sliderFaces[0]
$slide2 = $sliderFaces[1]
$hover = Ui-PickAt "$bSlider/$slide1 at"
Ui-Sync
Ui-Mark
Ui-Step "the mouse on $bSlider's $slide1" { [UiInput]::Move($script:UiWindow, $hover[0], $hover[1], 0x04) }
Ui-ExpectNew "BUILD hover pick: face $slide1 of $bSlider" 'the hover names the side a click picks'
No-Inspect 'Linear Part' 'the hover previews no slider (mitcad#508)'
Ui-Mark
Ui-Step "$bSlider's $slide1"             { Pick-Face $bSlider $slide1 }
Ui-ExpectNew 'Inspect Linear Part: place side' 'the slider on the side picked (On the side)'
Place 'Slider 5 test'
Refresh-Picks 'Linear Part'
Ui-Mark
Ui-Step "$bSlider's $slide2"             { Pick-Face $bSlider $slide2 }
Ui-ExpectNew 'Inspect Linear Part: place side' 'the slider on the other side picked (On the side)'
Place 'Slider 5 test'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Write-Host '--- 6b. Ends: an end of the third profile mitred and bored'
Ui-Step 'fit the view (F6)'              { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'Ends (F11)'                     { Ui-Key 'F11' }
Ui-ExpectNew 'Command Ends started' 'Ends started'
$ends3 = @(Faces-Of $b3 ':end_b') + @(Faces-Of $b3 ':end_a') + @(Faces-Of $b3 $side)
if ($ends3.Count -eq 0) { Ui-Fail "no face of the third profile ($b3) to pick" }
Ui-Step "the third profile's $($ends3[0])" { Pick-Face $b3 $ends3[0] }
Ui-ExpectNew 'Inspect Ends: ' "the part's ends"
Ui-Step 'a bore in the end'              { Ui-WheelLogged 'Panel Ends input machining' -1 }
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Ends of ' 'the end edited'
Ui-ExpectNew 'machining D6.8' 'with its bore'

Write-Host '--- 7. Save, reopen, Cut List'
Ui-Mark
Ui-Step 'save as (Ctrl+Shift+S)'         { Ui-Key 'ctrl+shift+s' }
Ui-FocusDialog '^Save As$'
Ui-Step 'the path'                       { Ui-Key 'ctrl+a'; Ui-Type $file; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew "Saved $fileLogged" 'saved'
Ui-StartApp 'build-reopened' @('--open', $file)
Ui-ExpectLog "Opened $fileLogged" 'reopened'
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD parts profile: 1 (1 in the design)' "the profile's source is in the design"
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Cut List (F9)'                  { Ui-Key 'F9' }
Ui-FocusDialog '^Cut List$'
Ui-ExpectNew 'Cut list row: ' "the cut list's rows"
Ui-ExpectNew 'x Profile 20 test' 'the profiles in it'
Ui-ExpectNew ' 45 deg (' 'the mitred end'
Ui-ExpectNew 'x Rail 15 test' 'the rail in it'
Ui-ExpectNew 'machining D6.8' 'the bored end'
$rows = Ui-LastMatch '^Cut list: (\d+) row\(s\)'
if ([int]"0$rows" -lt 3) { Ui-Fail "the cut list has $rows rows, not three or more" }
Ui-Mark
Ui-Step 'Copy as CSV (Alt+C)'            { Ui-Key 'alt+c' }
Ui-ExpectNew 'Build list CSV: Quantity,Part,Length (mm),End A,End B,Machining,Own features,Supplier,Part number,Mass each (kg)' "the CSV's header"
Ui-ExpectNew ',Test supplier,P20-T,' "the profiles' supplier and part number"

Ui-Step 'Save as PDF (Alt+P)'            { Ui-Key 'alt+p' }
Ui-FocusDialog '^Save Cut List as PDF$'
Ui-Step 'the PDF path'                   { Ui-Key 'ctrl+a'; Ui-Type $pdf; Ui-Key 'Return' }
Ui-FocusDialog '^Cut List$'
Ui-ExpectNew "Build list PDF written: $pdfLogged (" 'the PDF written'
Ui-ExpectNew 'Cut list of frame' "with the design's name"
$head = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($pdf)[0..4])
if ($head -ne '%PDF-') { Ui-Fail "$pdf is no PDF" }
Write-Host "ok   $pdf is a PDF"
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 8. The part picker (mitcad#418)'
# A 30 x 30 profile of I-type slot 6 on a 30 mm grid in the library's next
# version, fetched before the design is opened again.
$profile30 = @((Bar 30), (Frame 'slot_x+_1' '[15, 0, 0]' '[1, 0, 0]' '[0, 0, 1]'),
  (Frame 'slot_x-_1' '[-15, 0, 0]' '[-1, 0, 0]' '[0, 0, 1]'), (Frame 'slot_y+_1' '[0, 15, 0]' '[0, 1, 0]' '[0, 0, 1]'),
  (Frame 'slot_y-_1' '[0, -15, 0]' '[0, -1, 0]' '[0, 0, 1]')) -join "`n"
Design 'profiles\p30-test.mitcad' ($profile30 + $end)
$p30Entry = @'
    {"id": "p30-test", "path": "profiles/p30-test.mitcad", "name": "Profile 30 test", "category": "profiles",
     "supplier": "Test supplier", "part_number": "P30-T", "designation": "Profile 30 test L={L}",
     "build": {"role": "profile", "system": {"type": "I", "slot": 6, "grid": 30},
       "length": {"parameter": "L", "min": "10 mm", "max": "3000 mm", "stock": ["1000 mm"]},
       "section": {"width": "30 mm", "height": "30 mm"},
       "slot": {"depth": "8 mm", "opening": "6.2 mm", "inner_width": "16 mm"}, "mass_per_metre": "0.8 kg",
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "slot_x+_1", "kind": "slot"}, {"name": "slot_x-_1", "kind": "slot"},
                  {"name": "slot_y+_1", "kind": "slot"}, {"name": "slot_y-_1", "kind": "slot"}]}},
'@
$manifestPath = Join-Path $library 'mitcad-library.json'
$manifest = [System.IO.File]::ReadAllText($manifestPath)
$bracketEntry = '    {"id": "bracket-20-test"'
if (-not $manifest.Contains($bracketEntry)) { Ui-Fail "the manifest has no '$bracketEntry'" }
$manifest = $manifest.Replace('"library_version": "1.0.0"', '"library_version": "1.1.0"').Replace(
  $bracketEntry, $p30Entry + "`n" + $bracketEntry)
Set-Content -Encoding ascii $manifestPath $manifest
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'A 30 profile'), @('tag', 'v1.1.0'))) {
  $output = & $git.Source -C $library -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
Run-Cli 'library fetch of v1.1.0' @('library', 'fetch', $library)

Ui-StartApp 'build-picker' @('--open', $file)
Ui-ExpectLog "Opened $fileLogged" 'reopened'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD parts profile: 2 (1 in the design)' "the 30 profile listed beside the design's"
Ui-ExpectNew 'Panel Profile tile part: ' 'the Part tile (mitcad#427)'
Ui-Mark
Ui-Step 'Placement: on a plane'          { Ui-ClickLogged 'Panel Profile input place=plane' }
Expect-Inspect @('place grounded', '; part Profile ') 'a ground placement of the chosen profile'
Ui-Mark
Ui-Step 'the part picker (the Part tile)' { Ui-ClickLogged 'Panel Profile input part' }
Ui-ExpectNew 'Profile: Part button' 'the Part tile opens the picker'
Ui-ExpectNew 'Part picker opened: 2 parts for build.profile' 'the picker opened with the two profiles'
Ui-ExpectNew 'Part picker in this design: ' "the design's library parts read"
Ui-ExpectNew 'test-profiles/p20-test x' 'the placed 20 profiles in this design, with their count'
Ui-ExpectNew 'Part picker tile test-profiles/p30-test at ' "the tiles' places"
# The groups as a tree (mitcad#527): Slot profile (a category the others
# name as their parent) with its Profiles; a click on it lists its
# subgroups' parts; folded, its subgroup's row has no place, and unfolded
# it has one again.
Ui-ExpectNew 'Part picker groups: ' 'the groups logged'
$groupsTree = Ui-LastMatch '^Part picker groups: (.*)$'
# Each with its count (mitcad#1015).
if ("$groupsTree" -notlike '*All parts (2), Slot profile (2), Slot profile/Profiles (2)') {
  Ui-Fail "the groups are '$groupsTree', not Slot profile with its Profiles, two profiles each"
}
Write-Host "ok   the groups as a tree: $groupsTree"
Ui-ExpectNew 'Part picker place group fold Slot profile at ' "the fold's place"
Ui-Mark
Ui-Step 'the group Slot profile'         { Picker-Click 'Part picker group row Slot profile' }
Ui-ExpectNew 'Part picker group Slot profile: 2 parts' "the main group lists its subgroup's profiles"
Ui-Mark
Ui-Step 'fold Slot profile (its arrow)'  { Picker-Click 'Part picker place group fold Slot profile' }
Ui-ExpectNew 'Part picker group Slot profile folded' 'folded'
Ui-Mark
Ui-Step 'unfold it (its arrow)'          { Picker-Click 'Part picker place group fold Slot profile' }
Ui-ExpectNew 'Part picker group Slot profile unfolded' 'unfolded'
Ui-ExpectNew 'Part picker group row Slot profile/Profiles at ' "its subgroup's row again"
Ui-Mark
Ui-Step 'the group Profiles'             { Picker-Click 'Part picker group row Slot profile/Profiles' }
Ui-ExpectNew 'Part picker group Slot profile/Profiles: 2 parts' 'the group of profiles'
Ui-ExpectNew 'Part picker tile test-profiles/p30-test at ' "the tiles' places in the group"
Ui-Mark
Ui-Step 'a click on the 30 profile'      { Picker-Click 'Part picker tile test-profiles/p30-test' }
Ui-ExpectNew 'Part picker chose test-profiles/p30-test' 'chosen in the picker'
Expect-Inspect @('place grounded', '; part Profile 30 test') 'the preview uses the chosen profile at once'
# A star changes only the star (mitcad#517): the tiles and the groups stay
# where they are and the selection stays on the starred tile; the order
# applies when the list is made again.
$tilesBefore = $null
$groupsBefore = $null
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $tilesBefore -and $clock.ElapsedMilliseconds -lt 6000) {
  $tilesBefore = Ui-LastMatch '^Part picker tiles: (.*)$'
  if (-not $tilesBefore) { Start-Sleep -Milliseconds 50 }
}
# The groups by name: the kept line has no counts (mitcad#1015).
$groupsBefore = (Ui-LastMatch '^Part picker groups: (.*)$') -replace ' \(\d+\)', ''
# The 20 profile (with its uses in the design) then the 30 profile.
if ("$tilesBefore" -notmatch '^test-profiles/p20-test.*, test-profiles/p30-test$') {
  Ui-Fail "the tiles before the star are '$tilesBefore', not the 20 profile then the 30 profile"
}
Ui-Mark
Ui-Step 'star it (Star)'                 { Picker-Click 'Part picker place star' }
Ui-ExpectNew 'Part picker star p30-test on' 'the 30 profile starred'
Ui-ExpectNew "Part picker tiles kept: $tilesBefore; current test-profiles/p30-test" 'the tiles stay in their places, the selection on the starred tile'
Ui-ExpectNew "Part picker groups kept: $groupsBefore" 'the groups stay in their places'
if (Ui-NewLines | Where-Object { $_.StartsWith('Part picker group Slot profile/Profiles: ') }) {
  Ui-Fail 'a star made the tiles again'
}
Ui-Mark
Ui-Step 'All parts (the group row)'      { Picker-Click 'Part picker group row All parts' }
Ui-ExpectNew 'Part picker group All parts: ' 'another group'
Ui-Mark
Ui-Step 'Profiles again (the group row)' { Picker-Click 'Part picker group row Slot profile/Profiles' }
Ui-ExpectNew 'Part picker group Slot profile/Profiles: 2 parts, first test-profiles/p30-test' 'the starred profile first now'
# A group's star changes only the star too: its row stays where it is.
$groupsBefore = (Ui-LastMatch '^Part picker groups: (.*)$') -replace ' \(\d+\)', ''
Ui-Mark
Ui-Step 'star the group (its star)'      { Picker-Click 'Part picker place group star Slot profile/Profiles' }
Ui-ExpectNew 'Part picker group star slot profile/profiles on' 'the group Profiles starred'
Ui-ExpectNew "Part picker groups kept: $groupsBefore" 'the groups stay in their places'
Ui-Mark
Ui-Step 'its star off again'             { Picker-Click 'Part picker place group star Slot profile/Profiles' }
Ui-ExpectNew 'Part picker group star slot profile/profiles off' "the group's star off"
Ui-ExpectNew "Part picker groups kept: $groupsBefore" 'the groups stay in their places'
Ui-Mark
Ui-Step 'pin the picker'                 { Picker-Click 'Part picker place pin' }
Ui-ExpectNew 'Part picker pinned on' 'pinned'
Ui-Mark
Ui-Step 'close Profile (Escape)'         { Ui-Key 'Escape' }
Ui-ExpectNew 'Part picker stays pinned after build.profile' 'the pinned picker stays when Profile ends'
Ui-Mark
Ui-Step 'Connector (F8)'                 { Ui-Key 'F8' }
Ui-ExpectNew 'Part picker follows build.connector: ' 'the pinned picker follows Connector'
Ui-ExpectNew 'Part picker tile test-profiles/bracket-20-test at ' "the fitting connector's tile"
Ui-Mark
Ui-Step 'a click on the bracket'         { Picker-Click 'Part picker tile test-profiles/bracket-20-test' }
Ui-ExpectNew 'Part picker chose test-profiles/bracket-20-test' 'chosen in the picker'
Ui-ExpectNew 'Connector: Connector = Bracket 20 test' 'chosen in Connector'
Ui-Mark
Ui-Step 'unpin the picker'               { Picker-Click 'Part picker place pin' }
Ui-ExpectNew 'Part picker pinned off' 'unpinned'
Ui-Mark
Ui-Step 'close Connector (Escape)'       { Ui-Key 'Escape' }
Ui-ExpectNew 'Part picker closed with build.connector' 'the picker closes with the command'
Ui-Mark
Ui-Step 'Profile again (F3)'             { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD parts profile: 2 (1 in the design)' 'Profile again'
Ui-ExpectNew 'Panel Profile tile part: Profile 30 test' 'the profile chosen last in the Part tile'
Ui-Mark
Ui-Step 'Placement: on a plane'          { Ui-ClickLogged 'Panel Profile input place=plane' }
Expect-Inspect @('place grounded', '; part Profile 30 test') 'a ground placement of the 30 profile'
Ui-Mark
Ui-Step 'the part picker again (the tile)' { Ui-ClickLogged 'Panel Profile input part' }
Ui-ExpectNew 'Part picker opened: 2 parts for build.profile' 'the picker opened again'
Ui-ExpectNew 'Part picker group All parts: 1 parts, first test-profiles/p30-test' 'filtered to the system chosen last, I6 30 (mitcad#427)'
Ui-ExpectNew 'Part picker group row Favourites at ' "the groups' places"
Ui-Mark
Ui-Step 'Favourites'                     { Picker-Click 'Part picker group row Favourites' }
Ui-ExpectNew 'Part picker group Favourites: 1 parts, first test-profiles/p30-test' 'Favourites has the starred profile'
Place 'Profile 30 test'
# After Place no ghost (mitcad#513): Placement stays On a plane, but the
# next one is not previewed at the origin on top of the one placed there;
# the panel waits for a click.
Expect-Inspect @('failed: Click a plane or a face where the next one goes.') 'after Place the panel waits for a click'
Ui-ExpectNew 'View hint Profile: Click a plane or a face for the next one' 'the view asks for the next one''s plane'
Start-Sleep -Milliseconds 1500
Ui-Sync
$ghost = @(Ui-NewLines | Where-Object { $_.StartsWith('Inspect Profile: place grounded') })
if ($ghost.Count -gt 0) { Ui-Fail "a profile previewed at the origin after Place: $($ghost[0])" }
Write-Host 'ok   no profile previewed at the origin on top of the one placed'
Ui-ExpectNew 'test-profiles/p30-test x1' 'In this design has it after the placement'
Ui-ExpectNew 'Part picker group row In this design at ' "the groups' places again"
Ui-Mark
Ui-Step 'In this design'                 { Picker-Click 'Part picker group row In this design' }
Ui-ExpectNew 'Part picker group In this design: 1 parts, first test-profiles/p30-test' 'In this design lists the placed profile (System I6 30)'
Ui-Mark
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Part picker closed with build.profile' 'the picker closes with Profile'

# --- mitcad#452: the part a command starts with (part 9 of the Linux test).
Write-Host '--- 9. A new design starts with the first favourite (mitcad#452)'
# A second profile of the 30 system in the library's next version, starred
# beside the 30 profile starred in 8: two favourites of one system. The app
# stops first: the fetch replaces the library's files.
Ui-StopApp
Copy-Item (Join-Path $library 'profiles\p30-test.mitcad') (Join-Path $library 'profiles\p30h-test.mitcad')
$p30hEntry = $p30Entry.Replace('p30-test', 'p30h-test').Replace('Profile 30 test', 'Profile 30 heavy test').Replace(
  'P30-T', 'P30H-T')
$manifest = [System.IO.File]::ReadAllText($manifestPath)
if (-not $manifest.Contains($bracketEntry)) { Ui-Fail "the manifest has no '$bracketEntry'" }
$manifest = $manifest.Replace('"library_version": "1.1.0"', '"library_version": "1.2.0"').Replace(
  $bracketEntry, $p30hEntry + "`n" + $bracketEntry)
Set-Content -Encoding ascii $manifestPath $manifest
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'A heavy 30 profile'), @('tag', 'v1.2.0'))) {
  $output = & $git.Source -C $library -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
Run-Cli 'library fetch of v1.2.0' @('library', 'fetch', $library)
# The test library's names of the 30 profiles.
$p30Names = @{ 'p30-test' = 'Profile 30 test'; 'p30h-test' = 'Profile 30 heavy test' }

# A new design (no file); the stars of 8 are kept in the test's settings.
Ui-StartApp 'build-favourites'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'BUILD parts profile: 3 (0 in the design)' 'the three profiles of the library'
Ui-ExpectNew 'BUILD profile starts with test-profiles/p30-test: the first favourite' 'a new design starts with the starred profile, the only favourite'
Ui-ExpectNew 'Panel Profile tile part: Profile 30 test' 'its tile'
Ui-Mark
Ui-Step 'the part picker (the Part tile)' { Ui-ClickLogged 'Panel Profile input part' }
Ui-ExpectNew 'Part picker opened: 3 parts for build.profile' 'the picker with the three profiles'
Ui-ExpectNew 'Part picker shows the part chosen: test-profiles/p30-test' 'the picker shows the part chosen'
Ui-ExpectNew 'Part picker tile test-profiles/p30h-test at ' "the heavy profile's tile"
Ui-Mark
Ui-Step 'the heavy profile'              { Picker-Click 'Part picker tile test-profiles/p30h-test' }
Ui-ExpectNew 'Part picker chose test-profiles/p30h-test' 'chosen in the picker'
$tilesBefore = Ui-LastMatch '^Part picker tiles: (.*)$'
Ui-Mark
Ui-Step 'star it (Star)'                 { Picker-Click 'Part picker place star' }
Ui-ExpectNew 'Part picker star p30h-test on' 'a second favourite'
# The star changes only the star (mitcad#517): the tiles stay in their
# places, the selection on the starred one; the favourites come first once
# the list is made again (Favourites, then All parts).
Ui-ExpectNew "Part picker tiles kept: $tilesBefore; current test-profiles/p30h-test" 'the tiles stay where they are, the selection on the starred tile'
Ui-Mark
Ui-Step 'Favourites (the group row)'     { Picker-Click 'Part picker group row Favourites' }
Ui-ExpectNew 'Part picker group Favourites: 2 parts, first test-profiles/p30' 'the two favourites'
# The first favourite in the picker's order (the catalog's among the
# starred), from the group's line since the star, and the other one.
$first = $null
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $first -and $clock.ElapsedMilliseconds -lt 6000) {
  $first = Ui-NewLines |
    ForEach-Object { if ($_ -match '^Part picker group Favourites: 2 parts, first test-profiles/(p30[a-z]*-test)$') { $Matches[1] } } |
    Select-Object -Last 1
  if (-not $first) { Start-Sleep -Milliseconds 50 }
}
Ui-Mark
Ui-Step 'All parts (the group row)'      { Picker-Click 'Part picker group row All parts' }
Ui-ExpectNew "Part picker group All parts: 3 parts, first test-profiles/$first" 'the favourites first'
switch ("$first") {
  'p30-test' { $second = 'p30h-test' }
  'p30h-test' { $second = 'p30-test' }
  default { Ui-Fail "the picker's first tile is no 30 profile ('$first')" }
}
Write-Host "ok   the first favourite is $first, the second $second"
Ui-Mark
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Part picker closed with build.profile' 'the picker closes with Profile'
Ui-Mark
Ui-Step 'Profile again (F3)'             { Ui-Key 'F3' }
Ui-ExpectNew "BUILD profile starts with test-profiles/${first}: the first favourite" 'the design has no profile yet: the first favourite of the system'
Choose-InPicker 'Profile' "test-profiles/$second"
Place $p30Names[$second]
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Profile in the same design (F3)' { Ui-Key 'F3' }
Ui-ExpectNew "BUILD profile starts with test-profiles/${second}: " 'the part placed last in this design'
Ui-ExpectNew "Panel Profile tile part: $($p30Names[$second])" 'its tile'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
# The message boxes titled "MitCad" are new windows (Ui-FocusDialog).
$before = Ui-Windows '^MitCad$'
Ui-Mark
Ui-Step 'new design (Ctrl+N)'            { Ui-Key 'ctrl+n' }
Ui-FocusDialog '^MitCad$' -Except $before
Ui-Step "don't save (D)"                 { Ui-Key 'd' }
Ui-FocusMain
Ui-ExpectNew 'New document' 'a new design'
Ui-Mark
Ui-Step 'Profile in the new design (F3)' { Ui-Key 'F3' }
Ui-ExpectNew "BUILD profile starts with test-profiles/${first}: the first favourite" 'the first favourite again, not the part of the earlier design'
Ui-ExpectNew "Panel Profile tile part: $($p30Names[$first])" 'its tile'
Ui-Mark
Ui-Step 'the part picker (the Part tile)' { Ui-ClickLogged 'Panel Profile input part' }
Ui-ExpectNew "Part picker shows the part chosen: test-profiles/$first" 'the picker shows and highlights it'
Ui-Mark
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Part picker closed with build.profile' 'the picker closes with Profile'

# mitcad#451: Shift + click on a placement's pick input is the pick and
# Place at once (InputDef::withShiftClickApplies), as part 10 of the Linux
# test: in a new design a Shift + click on the XY plane places the first
# profile without Place, a plain click on its side only previews, a Shift +
# click there places the next; with a typed length over the part's longest
# a Shift + click is refused, places nothing and the panel says why. The
# clicks carry Shift in the keyboard state and the button messages
# (Ui-ClickWith).
Write-Host '--- 10. Shift + click places at once (mitcad#451)'
function Shift-PickFace([string]$Body, [string]$Face) {
  $at = Ui-PickAt "$Body/$Face at"
  Ui-ClickWith 'shift' $at[0] $at[1]
}
# No-Placement description: no "Placed ..." line since the mark, once the
# app has handled the input.
function No-Placement([string]$Description) {
  Start-Sleep -Seconds 1
  Ui-Sync
  $placed = Ui-NewLines | Where-Object { $_.StartsWith('Placed ') } |
    Select-Object -First 1
  if ($placed) { Ui-Fail "${Description}: $placed" }
  Write-Host "ok   $Description"
}
Ui-StartApp 'build-shift-click'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'Command Profile started' 'Profile started'
Ui-ExpectNew 'View hint Profile: Click a plane or a face, or press Enter; Shift+click places at once' "the view's hint names Shift+click"
# Shorter than the stock length, as in part 3: its sides stay in reach of
# a click in session 0's smaller window.
Ui-Step 'a typed length of 400'          { Ui-TypeIn 'Panel Profile input length' '400' }
Ui-Mark
Ui-Step 'Shift+click on the XY plane'    { $at = Ui-LoggedAt 'Datum xy'; Ui-ClickWith 'shift' $at[0] $at[1] }
Ui-ExpectNew 'Profile Place On: Shift+click places at once' 'a Shift+click on the plane'
Ui-ExpectNew 'Placed Profile ' 'the first profile placed without Place' 30
Ui-ExpectNew 'Applied Profile (1)' 'the panel stays for the next one'
$b10 = Placed-Body
if (-not $b10) { Ui-Fail 'no body of the profile placed by Shift+click' }
Refresh-Picks 'Profile'
$sides10 = @(Faces-Of $b10 $side)
if ($sides10.Count -eq 0) { Ui-Fail "no side of the profile ($b10) to pick" }
Ui-Mark
Ui-Step 'a plain click on its side'      { Pick-Face $b10 $sides10[0] }
Ui-ExpectNew 'Inspect Profile: place side' 'a plain click previews on the side'
No-Placement 'a plain click only previews'
Ui-Mark
Ui-Step 'Shift+click on its side'        { Shift-PickFace $b10 $sides10[0] }
Ui-ExpectNew 'Profile Place On: Shift+click places at once' 'a second Shift+click'
Ui-ExpectNew 'Placed Profile ' 'the next profile placed on the side without Place' 30
Ui-ExpectNew 'Applied Profile (2)' 'the panel stays for the next one'
# A placement the model refuses: a length over the part's 3000 mm.
Ui-Step 'a typed length of 5000'         { Ui-TypeIn 'Panel Profile input length' '5000' }
Refresh-Picks 'Profile'
$sides10 = @(Faces-Of $b10 $side)
if ($sides10.Count -eq 0) { Ui-Fail "no side of the profile ($b10) to pick again" }
Ui-Mark
Ui-Step 'Shift+click on its side again'  { Shift-PickFace $b10 $sides10[0] }
Ui-ExpectNew 'Profile Place On: Shift+click places at once' 'a Shift+click that cannot place'
Ui-ExpectNew 'Profile: OK refused' 'the placement refused'
Ui-ExpectNew 'Inspect Profile: failed: ' 'the panel shows why'
Ui-ExpectNew "longer than the part's longest, 3000 mm" "the reason: the length is over the part's longest"
No-Placement 'a failed Shift+click makes nothing'
Ui-Mark
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Closed Profile after 2' 'Close keeps the two'

Ui-Finish 'UI Windows BUILD test'
