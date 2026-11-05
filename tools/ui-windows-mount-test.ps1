# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MountCommands.cpp app/commands/JointCommands.cpp app/library app/files tools/cli
# Input-driven Windows UI test of mount frames of any part and Place Part
# (mitcad#525) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted); the Linux test is tools/ui-mount-test.sh. Synthetic parts
# mitcad-cli makes (no model is downloaded) in local git repositories (no
# network); skipped where git is not installed (fetching needs it):
#   1. Gridfinity: a baseplate of 3 x 2 cells of 42 mm (each cell a block of
#      its own) dropped in as a copy with Enter, a 1 x 1 bin on cell (2, 1),
#      a 2 x 1 bin turned 90 degrees refused on (2, 0) and (2, 1) (one is
#      taken) and placed on (0, 0) and (0, 1), a 1 x 1 bin on (1, 0);
#      saved, the bins where their
#      cells are. With MITCAD_GRIDFINITY_LIBRARY (a ready-made model of the
#      test files as a library with its frames) its parts too; skipped
#      without.
#   2. Free on a face: a lamp Shift+clicked onto the plate's top, a planar
#      joint; moving the plate carries it, a drag slides it on the face.
#   3. An own screw: Joint Origin's Mount Frame (into a hole, turns and
#      slides) under its head, published to a library of one's own, placed
#      into the plate's hole: coaxial, a cylindrical joint.
#   4. A drop-in joined afterwards with Joint by its mount frame (the
#      browser's row of its joint origin) onto a plate's top, in a design
#      of its own.
#   5. A selection made before (mitcad#1197): a hole's rim selected, then
#      Insert from Library > an ISO 4762 M5x16 screw with the parts
#      library's mount frame (`seat`, a shaft frame under the head; a
#      synthetic design here, the Linux test takes the generator's):
#      Place Component starts with the rim as Place On, the screw shown in
#      the hole; Enter places it, head on the face, a rigid joint to the
#      hole; no other screw at the origin.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-mount-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-mount'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Any error of the script fails the test and stops the app: an app left
# running keeps ctest's output pipe open, and ctest waited for its time-out.
trap { Ui-Fail "the test script stopped: $_ $($_.InvocationInfo.PositionMessage)" }

Ui-Init $App $Out $QtBin @{
  'assemble.joint_origin' = 'F7'; 'tools.libraries' = 'F8'; 'insert.library_component' = 'F9'
  'assemble.joint' = 'F11'; 'tools.publish_library' = 'F12'; 'solid.fillet' = 'Shift+F5' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows mount test (skipped)'
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
$env:GIT_AUTHOR_NAME = 'Mount Tester'
$env:GIT_AUTHOR_EMAIL = 'tester@example.invalid'
$env:GIT_COMMITTER_NAME = 'Mount Tester'
$env:GIT_COMMITTER_EMAIL = 'tester@example.invalid'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'

# Design name commands: a design from mitcad-cli's commands (JSON text).
function Design([string]$Name, [string]$Commands) {
  $script = Join-Path $work "$Name.json"
  Set-Content -Encoding ascii $script "[$Commands]"
  $file = Join-Path $work "$Name.mitcad"
  $output = & $cli run $script --save $file
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path $file)) { Ui-Fail "mitcad-cli run ${Name}: $($output -join ' / ')" }
  return $file
}
function Box([double]$X, [double]$Y, [double]$L, [double]$W, [double]$H) {
  return ('{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [' + "$X, $Y" + '], "length": ' +
          "$L, " + '"width": ' + "$W, " + '"height": ' + "$H" + ', "operation": "new_body"}}')
}
function Frame([string]$Origin, [string]$Normal, [string]$Mount) {
  return ('{"cmd": "add_feature", "def": {"type": "joint_origin", "geometry": {"origin": ' + $Origin + ', "normal": ' +
          $Normal + ', "x_axis": [1, 0, 0]}, "mount": ' + $Mount + '}}')
}

$cells = @((Box 0 0 42 42 5), (Box 42 0 42 42 5), (Box 84 0 42 42 5), (Box 0 42 42 42 5), (Box 42 42 42 42 5),
           (Box 84 42 42 42 5)) -join ', '
Design 'baseplate' ($cells + ', ' + (Frame '[0, 0, 5]' '[0, 0, 1]' '{"name": "grid", "kind": "grid", "pitch": [42, 42], "cells": [3, 2]}')) | Out-Null
Design 'bin1x1' ((Box 0 0 42 42 21) + ', ' +
                 (Frame '[21, 21, 0]' '[0, 0, -1]' '{"name": "foot", "kind": "foot", "footprint": [1, 1], "pitch": [42, 42]}')) | Out-Null
Design 'bin2x1' ((Box 0 0 84 42 21) + ', ' +
                 (Frame '[42, 21, 0]' '[0, 0, -1]' '{"name": "foot", "kind": "foot", "footprint": [2, 1], "pitch": [42, 42]}')) | Out-Null
Design 'lamp' ((Box 0 0 10 10 5) + ', ' +
               (Frame '[5, 5, 0]' '[0, 0, -1]' '{"name": "base", "kind": "face", "joint": "planar"}')) | Out-Null
$screw = Design 'screw' @'
{"cmd": "add_feature", "def": {"type": "cylinder", "plane": {"origin": [0, 0, -3], "normal": [0, 0, 1]},
  "center": [0, 0], "diameter": 14, "height": 3, "operation": "new_body"}},
{"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 8, "height": 20,
  "operation": "join", "participants": ["F1.b0"]}}
'@
$scene = Design 'scene' ('{"cmd": "create_component", "name": "Plate"}, ' + (Box 200 0 100 60 10) + ', ' + @'
{"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [270, 30], "diameter": 10, "height": 10,
  "operation": "cut", "participants": ["F1.b0"]}},
{"cmd": "activate_component", "component": "Root"}
'@)
# The plate a dropped-in lamp is joined to (4.): Plate:1, its top in sight.
$joinPlate = Design 'join' ('{"cmd": "create_component", "name": "Plate"}, ' + (Box 20 20 100 60 10) +
                            ', {"cmd": "activate_component", "component": "Root"}')
# 5.: an ISO 4762 M5x16 as the parts library's: a head of 8.5 x 5 on z = 0,
# a shank of 5 x 16 down, its mount frame `seat` at the origin with z down
# the shank (make-fastener-library.py's); a plate with a hole of 5.5 mm,
# Plate:1, 100 x 60 x 10 at (20, 20), the hole at (70, 50) (F1.b0: its top
# F1:top, the hole's wall F2:side0, the rim their edge).
Design 'iso4762' ('{"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 8.5, ' +
                  '"height": 5, "operation": "new_body"}}, ' +
                  '{"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0], "diameter": 5, ' +
                  '"height": -16, "operation": "join", "participants": ["F1.b0"]}}, ' +
                  (Frame '[0, 0, 0]' '[0, 0, -1]' '{"name": "seat", "kind": "shaft"}')) | Out-Null
$holed = Design 'holed' ('{"cmd": "create_component", "name": "Plate"}, ' + (Box 20 20 100 60 10) + ', ' + @'
{"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [70, 50], "diameter": 5.5, "height": 10,
  "operation": "cut", "participants": ["F1.b0"]}},
{"cmd": "activate_component", "component": "Root"}
'@)

$library = Join-Path $work 'mounts'
$mine = Join-Path $work 'screws'
$output = & $cli library init $library --id test-mounts --name 'Test mounts' --license CC0-1.0 --author 'Mount Tester'
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library init: $($output -join ' / ')" }
foreach ($part in @(@('baseplate', 'Baseplate', 'baseplates'), @('bin1x1', 'Bin 1x1', 'bins'), @('bin2x1', 'Bin 2x1', 'bins'),
                    @('lamp', 'Lamp', 'lamps'))) {
  $output = & $cli library add $library (Join-Path $work "$($part[0]).mitcad") --id $part[0] --name $part[1] `
    --designation $part[1] --category $part[2]
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library add $($part[0]): $($output -join ' / ')" }
}
$output = & $cli library check $library
if ($LASTEXITCODE -ne 0) { Ui-Fail "the library has errors: $($output -join ' / ')" }
if (-not (Select-String -Path (Join-Path $library 'mitcad-library.json') -Pattern '"kind": "foot"' -Quiet)) {
  Ui-Fail 'library add wrote no mounts'
}
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'Mount parts'), @('tag', 'v1.0.0'))) {
  $output = & $git.Source -C $library -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
Write-Host 'ok   the library of the synthetic parts, its mounts from the designs'

# The parts library's screw (5.), in a library of its own: its frame from
# the design, as the parts library's manifest lists it.
$parts = Join-Path $work 'parts'
$output = & $cli library init $parts --id test-iso --name 'Test ISO parts' --license CC0-1.0 --author 'Mount Tester'
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library init: $($output -join ' / ')" }
$output = & $cli library add $parts (Join-Path $work 'iso4762.mitcad') --id iso4762 `
  --name 'ISO 4762 M5x16' --designation 'ISO 4762 M5x16' --standard 'ISO 4762' --category screws
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library add iso4762: $($output -join ' / ')" }
$output = & $cli library check $parts
if ($LASTEXITCODE -ne 0) { Ui-Fail "the ISO parts have errors: $($output -join ' / ')" }
if (-not (Select-String -Path (Join-Path $parts 'mitcad-library.json') -Pattern '"name": "seat"' -Quiet)) {
  Ui-Fail 'the screw lists no mount frame'
}
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'ISO parts'), @('tag', 'v1.0.0'))) {
  $output = & $git.Source -C $parts -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
Write-Host 'ok   the ISO 4762 screw lists its mount frame'

# Check "description" steps...: the saved scene opened by mitcad-cli with
# the script's expectations.
function Check([string]$Description, [string[]]$Steps, [string]$File = $scene) {
  $script = Join-Path $work 'check.json'
  Set-Content -Encoding ascii $script ('[' + ($Steps -join ', ') + ']')
  $output = & $cli run $script --open $File
  if ($LASTEXITCODE -ne 0) { Ui-Fail "${Description}: $($output -join ' / ')" }
  Write-Host "ok   $Description"
}

function Save([string]$File = $scene) {
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
  # The app logs the path with forward slashes.
  Ui-ExpectNew ('Saved ' + $File.Replace('\', '/')) 'saved'
}

function Add-Source([string]$Path) {
  Ui-Step 'Add URL (Alt+A)'                { Ui-Key 'alt+a' }
  Ui-FocusDialog '^Add Library$'
  Ui-Step 'the folder'                     { Ui-Type $Path; Ui-Key 'Return' }
  Ui-FocusDialog '^Library Sources$'
  Ui-ExpectNew "Library source added: $Path" 'the source added'
}

# Close-Libraries: Close (Alt+C) in the Library Sources dialog, which has the
# keys. Its button clicks a moment after the key (Qt's animateClick) and
# nothing is logged for it: the main window takes keys again once the
# dialog's window is gone.
function Close-Libraries {
  $dialog = $script:UiTarget
  Ui-Step 'close (Alt+C)'                  { Ui-Key 'alt+c' }
  # Looked for every 20 ms (200 ms before, mitcad#870), up to 10 s.
  Ui-WaitGone $dialog 10000
  if ((Ui-Windows '^Library Sources$') -contains $dialog) { Ui-Fail 'the Library Sources dialog did not close' }
  Ui-FocusMain
}

# Insert "search" id [-Copy]: Insert from Library, the part searched (its
# id), linked or a copy, Insert: Place Part starts for a part with frames.
function Insert([string]$Search, [string]$Id, [switch]$Copy) {
  Ui-Mark
  Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
  Ui-FocusDialog '^Browse Components$'
  Ui-Step "search $Search"                 { Ui-Key 'alt+s'; Ui-Type $Search }
  Ui-ExpectNew "Library search '$Search': 1 components" 'the search finds it'
  Ui-ExpectNew "Library browser selected test-mounts/${Id}: version " 'the part selected'
  if ($Copy) { Ui-Step 'a copy (Alt+P)'   { Ui-Key 'alt+p' } }
  Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
  Ui-FocusMain
  Ui-ExpectNew 'Command Place Component started' 'Place Part started'
}

# The bodies (and the frame's place) a placement or a drop-in logged since the mark.
function Since-Mark([string]$Text) { return @(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) }) }
# (The comma keeps an array of one an array.)
function Bodies-Of([string]$Text) {
  $line = Since-Mark $Text | Select-Object -Last 1
  if ("$line" -notmatch '; bodies ([^;]*)') { Ui-Fail "no bodies logged for '$Text'" }
  return , @($Matches[1] -split ', ')
}
function At-Of([string]$Text) {
  $line = Since-Mark $Text | Select-Object -Last 1
  if ("$line" -notmatch ' at \(([^)]*)\); bodies ') { Ui-Fail "no place logged for '$Text'" }
  return , @($Matches[1] -split ', ' | ForEach-Object { [double]::Parse($_, [Globalization.CultureInfo]::InvariantCulture) })
}
# Cell body: a face of a cell to click, as the last "Pick places:" logs
# them: its top, else its front (side0, -y), else any (a click anywhere on
# a cell takes the cell; a bin standing on it hides its top).
function Cell([string]$Body) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $places = @($lines[$start..($lines.Count - 1)] | Where-Object { $_.StartsWith("Pick face $Body/") })
  $stem = $Body -replace '\.b0$', ''
  foreach ($face in @('top', 'side0')) {
    if ($places | Where-Object { $_.StartsWith("Pick face $Body/${stem}:$face at ") }) { return "face $Body/${stem}:$face" }
  }
  if ($places.Count -eq 0) {
    Ui-Fail "no face of the cell $Body among the pick places: $(($lines[$start..($lines.Count - 1)] | Where-Object { $_.StartsWith('Pick ') }) -join ' / ')"
  }
  return "face $Body/"
}
# Pick-Of "regex": the item ("face F1.b0/F2:side0") of the first pick place
# after the last "Pick places:" whose item matches.
function Pick-Of([string]$Pattern) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  foreach ($line in $lines[$start..($lines.Count - 1)]) {
    if ($line -notmatch '^Pick (.+) at \d+,\d+$') { continue }
    # The item before the second match, which sets $Matches again.
    $item = $Matches[1]
    if ($item -match $Pattern) { return $item }
  }
  return $null
}
# Browser-Reveal "Browser <path>": wheel steps down the browser until the
# row is in sight (session 0's window is small: rows below the ones in
# sight are not logged), as tools/ui-windows-browser-test.ps1 does.
function Row-InSight([string]$Text) {
  $last = "$(Ui-LogLines | Where-Object { $_.Contains("$Text at ") -or $_.EndsWith("$Text out of sight") } |
    Select-Object -Last 1)"
  return ($last.Length -gt 0) -and -not $last.EndsWith(' out of sight')
}
function Browser-Reveal([string]$Text) {
  for ($i = 0; $i -lt 20 -and -not (Row-InSight $Text); $i++) {
    $line = "$(Ui-LogLines | Where-Object { $_ -match '^Browser (eye |radio )?Root.* at \d+,\d+$' } | Select-Object -Last 1)"
    if ($line -notmatch ' at (\d+),(\d+)$') { Ui-Fail 'no browser row in sight' }
    $x = [int]$Matches[1]; $y = [int]$Matches[2]
    [UiInput]::Move($script:UiWindow, $x, $y, 0x04)
    Start-Sleep -Milliseconds 80
    [UiInput]::Wheel($script:UiWindow, $x, $y, -3)
    Ui-Sync
  }
  if (-not (Row-InSight $Text)) { Ui-Fail "'$Text' cannot be scrolled into sight" }
}
function Number([double]$Value) { return $Value.ToString('R', [Globalization.CultureInfo]::InvariantCulture) }

Ui-StartApp 'mount' @('--open', $scene)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }

Write-Host '--- 0. Library Sources: the synthetic parts'' library only'
Ui-Mark
Ui-Step 'Library Sources (F8)'             { Ui-Key 'F8' }
Ui-FocusDialog '^Library Sources$'
Ui-Step 'remove a default (Alt+R)'         { Ui-Key 'alt+r' }
Ui-Step 'and the other (Alt+R)'            { Ui-Key 'alt+r' }
Add-Source $library
Ui-Step 'Fetch All (Alt+F)'                { Ui-Key 'alt+f' }
Ui-ExpectNew "Library fetched: library Test mounts (test-mounts) from $library" 'fetched' 30
Close-Libraries

Write-Host '--- 1. Gridfinity: a baseplate dropped in, bins on its cells'
Insert 'Baseplate' 'baseplate' -Copy
Ui-ExpectNew 'Insert from Library: test-mounts/baseplate has mount frames grid (a grid, fixed): Place Part' 'Place Part'
Ui-Mark
Ui-Step 'drop it in (Enter)'               { Ui-Key 'Return' }
Ui-ExpectNew 'Inserted component Baseplate (copy) from test-mounts v1.0.0 (' 'dropped in as before' 30
Ui-ExpectNew 'mount frames grid (grid)' 'its grid a frame of its component'
$cellBodies = Bodies-Of 'Inserted component Baseplate'
if ($cellBodies.Count -ne 6) { Ui-Fail "the baseplate's six cells: $($cellBodies -join ' ')" }
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
# The scene was fitted before the baseplate came in at the origin, out of
# the view: fitted again, its cells' faces are logged where they are.
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }

# The view looks from the front right (-y, +x): the bins stand where they
# hide no cell clicked later. A taken cell is under its bin, out of sight;
# the long bin, turned, is refused on the free cell (2, 0) in front of the
# first bin, as it would cover (2, 1) too. A foot takes faces only: the
# planes of the frames of the bins placed lie in the cells' tops.
Insert 'Bin 1x1' 'bin1x1'
Ui-ExpectNew ': frames foot (on a grid, fixed)' "the bin's foot frame"
Ui-Mark
Ui-Step 'click cell (2, 1)'                { Ui-ClickPick (Cell $cellBodies[5]) }
Ui-ExpectNew 'Inspect Place Component: place part Bin 1x1: frame foot (foot), rigid joint, on Baseplate:1, cell (2, 1)' `
  'the foot snaps to the cell clicked' 20
Ui-Mark
Ui-Step 'Place (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'by its frame foot on Baseplate:1, cell (2, 1): rigid joint' 'the bin placed on its cell' 30
$bin1 = (Bodies-Of 'Placed part Bin 1x1')[0]
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }

Insert 'Bin 2x1' 'bin2x1'
Ui-Step 'Rotate 90'                        { Ui-TypeIn 'Panel Place Component input rotate' '90' }
Ui-Mark
Ui-Step 'click cell (2, 0)'                { Ui-ClickPick (Cell $cellBodies[2]) }
Ui-ExpectNew "the cell (2, 1) of Baseplate:1's grid is taken by Bin 1x1:1" `
  'a taken cell is refused with who stands there' 20
Ui-Mark
Ui-Step 'click cell (0, 1)'                { Ui-ClickPick (Cell $cellBodies[3]) }
Ui-ExpectNew 'on Baseplate:1, cells (0, 0) and (0, 1)' 'turned, it takes two cells along Y' 20
Ui-Mark
Ui-Step 'Place (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Placed part Bin 2x1 (linked) from test-mounts' 'the long bin placed' 30
$bin2 = (Bodies-Of 'Placed part Bin 2x1')[0]
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }

Insert 'Bin 1x1' 'bin1x1'
Ui-Mark
Ui-Step 'click cell (1, 0)'                { Ui-ClickPick (Cell $cellBodies[1]) }
Ui-ExpectNew 'on Baseplate:1, cell (1, 0)' 'a free cell' 20
Ui-Mark
Ui-Step 'Place (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Placed part Bin 1x1 (linked) from test-mounts' 'the second bin placed' 30
$bin3 = (Bodies-Of 'Placed part Bin 1x1')[0]
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
Save
Check 'the bins stand on their cells, the long one turned' @(
  "{`"expect`": {`"occurrence`": `"Bin 1x1:1`", `"body`": `"$bin1`", `"center`": [105, 63, 15.5], `"tolerance`": 1e-6}}",
  "{`"expect`": {`"occurrence`": `"Bin 2x1:1`", `"body`": `"$bin2`", `"center`": [21, 42, 15.5], `"tolerance`": 1e-6}}",
  "{`"expect`": {`"occurrence`": `"Bin 1x1 (1):1`", `"body`": `"$bin3`", `"center`": [63, 21, 15.5], `"tolerance`": 1e-6}}")

if ($env:MITCAD_GRIDFINITY_LIBRARY -and (Test-Path (Join-Path $env:MITCAD_GRIDFINITY_LIBRARY 'mitcad-library.json'))) {
  Write-Host '--- 1b. A ready-made Gridfinity model of the test files'
  $manifest = Get-Content -Raw (Join-Path $env:MITCAD_GRIDFINITY_LIBRARY 'mitcad-library.json') | ConvertFrom-Json
  $gridPart = $manifest.components | Where-Object { $_.mounts | Where-Object { $_.kind -eq 'grid' } } | Select-Object -First 1
  $footPart = $manifest.components | Where-Object { $_.mounts | Where-Object { $_.kind -eq 'foot' } } | Select-Object -First 1
  if (-not $gridPart -or -not $footPart) { Ui-Fail 'the test files'' library has no grid part or no foot part' }
  Ui-Mark
  Ui-Step 'Library Sources (F8)'           { Ui-Key 'F8' }
  Ui-FocusDialog '^Library Sources$'
  Add-Source $env:MITCAD_GRIDFINITY_LIBRARY
  Ui-Step 'Fetch All (Alt+F)'              { Ui-Key 'alt+f' }
  Ui-ExpectNew "Library fetched: library $($manifest.name) ($($manifest.id))" "the model's library fetched" 60
  Close-Libraries
  Ui-Mark
  Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
  Ui-FocusDialog '^Browse Components$'
  Ui-Step "search $($gridPart.name)"       { Ui-Key 'alt+s'; Ui-Type $gridPart.name }
  Ui-Step 'a copy (Alt+P)'                 { Ui-Key 'alt+p' }
  Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
  Ui-FocusMain
  Ui-ExpectNew 'Command Place Component started' "Place Part with the model's baseplate"
  Ui-Mark
  Ui-Step 'drop it in (Enter)'             { Ui-Key 'Return' }
  Ui-ExpectNew "Inserted component $($gridPart.name) (copy)" "the model's baseplate dropped in" 60
  $gridBody = (Bodies-Of "Inserted component $($gridPart.name)")[0]
  Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
  Ui-Mark
  Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
  Ui-FocusDialog '^Browse Components$'
  Ui-Step "search $($footPart.name)"       { Ui-Key 'alt+s'; Ui-Type $footPart.name }
  Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
  Ui-FocusMain
  Ui-ExpectNew 'Command Place Component started' "Place Part with the model's bin"
  Ui-Mark
  Ui-Step 'click the baseplate'            { Ui-ClickPick "face $gridBody/" }
  Ui-ExpectNew "Inspect Place Component: place part $($footPart.name): frame " "the model's bin on the model's grid" 60
  Ui-Mark
  Ui-Step 'Place (Enter)'                  { Ui-Key 'Return' }
  Ui-ExpectNew "Placed part $($footPart.name) (linked)" "the model's bin placed" 60
  Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
} else {
  Write-Host 'skip the ready-made Gridfinity model (MITCAD_GRIDFINITY_LIBRARY is not set)'
}

Write-Host '--- 2. Free on a face: a lamp on the plate''s top'
Insert 'Lamp' 'lamp'
Ui-ExpectNew ': frames base (on a face, free on the face)' "the lamp's frame"
Ui-Mark
$at = Ui-PickAt 'face F1.b0/F1:top'
# Shift + click: the pick and Place in one (as BUILD's, mitcad#451).
Ui-Step "Shift+click on the plate's top"   { Ui-ClickWith 'shift' $at[0] $at[1] }
Ui-ExpectNew 'Inspect Place Component: place part Lamp: frame base (face), planar joint, on Plate:1' `
  'the lamp lies where clicked, free on the face' 20
Ui-ExpectNew 'Placed part Lamp (linked) from test-mounts' 'Shift+click placed it at once' 30
$lamp = (Bodies-Of 'Placed part Lamp')[0]
$l = At-Of 'Placed part Lamp'
if ([Math]::Abs($l[2] - 10) -gt 1e-9) { Ui-Fail "the lamp's frame is on the plate's top (z 10), not $($l[2])" }
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
Save
$lx = Number $l[0]; $ly = Number $l[1]
$mx = Number ($l[0] + 10); $my = Number ($l[1] + 5)
Check 'the lamp is free on the plate''s top: carried by the plate, slid by a drag' @(
  "{`"expect`": {`"occurrence`": `"Lamp:1`", `"body`": `"$lamp`", `"center`": [$lx, $ly, 12.5], `"tolerance`": 1e-3}}",
  '{"expect": {"query": {"query": "joint_dof", "occurrence": "Lamp:1"}, "result": {"dof": 3}}}',
  '{"cmd": "set_occurrence_transform", "occurrence": "Plate:1", "transform": {"translation": [0, 0, 50]}}',
  "{`"expect`": {`"occurrence`": `"Lamp:1`", `"body`": `"$lamp`", `"center`": [$lx, $ly, 62.5], `"tolerance`": 1e-3}}",
  # Grounded, the plate stays where it is; free, the drag moved it with the lamp.
  '{"cmd": "ground_occurrence", "occurrence": "Plate:1"}',
  "{`"cmd`": `"drag_occurrence`", `"occurrence`": `"Lamp:1`", `"point`": [$lx, $ly, 60], `"target`": [$mx, $my, 90]}",
  "{`"expect`": {`"occurrence`": `"Lamp:1`", `"body`": `"$lamp`", `"center`": [$mx, $my, 62.5], `"tolerance`": 1e-2}}")

Write-Host '--- 3. An own screw: a mount frame under its head, published, placed into the hole'
Ui-StartApp 'mount-screw' @('--open', $screw)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'Joint Origin (F7)'                { Ui-Key 'F7' }
Ui-ExpectNew 'Command Joint Origin started' 'Joint Origin started'
Ui-Step "the head's top, where it bears"   { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-Step 'Mount Frame on'                   { Ui-ClickLogged 'Panel Joint Origin input mount' }
Ui-Step 'its name'                         { Ui-TypeIn 'Panel Joint Origin input mount_name' 'head' }
# A drop-down's list cannot open in session 0: wheel steps choose, one
# message a step (a combo box moves one entry a message, whatever its
# delta: -2 in one message chose Turns, not Turns and slides).
Ui-Step 'into a hole'                      { Ui-WheelLogged 'Panel Joint Origin input mount_kind' -1 }
Ui-Step 'turns and slides'                 {
  Ui-WheelLogged 'Panel Joint Origin input mount_joint' -1
  # The first step handled (mitcad#870: a fixed 150 ms before).
  Ui-Sync
  Ui-WheelLogged 'Panel Joint Origin input mount_joint' -1
}
Ui-Mark
Ui-Step 'OK (Enter)'                       { Ui-Key 'Return' }
Ui-ExpectNew 'Added joint origin JointOrigin1: origin (0, 0, 0), normal (0, 0, 1), mount head (shaft, cylindrical)' `
  'a mount frame under the head, z along the shank'
Ui-Mark
Ui-Step 'Publish to Library (F12)'         { Ui-Key 'F12' }
Ui-FocusDialog '^Publish to Library$'
Ui-Step 'the folder'                       { Ui-Key 'alt+f'; Ui-Type $mine }
Ui-Step "the library's id"                 { Ui-Key 'alt+i'; Ui-Type 'my-screws' }
Ui-Step "the library's name"               { Ui-Key 'alt+n'; Ui-Type 'My screws' }
Ui-Step 'the author'                       { Ui-Key 'alt+a'; Ui-Type 'Mount Tester' }
Ui-Step "the component's id"               { Ui-Key 'alt+d'; Ui-Type 'screw-m8' }
Ui-Step 'its name'                         { Ui-Key 'alt+m'; Ui-Type 'Screw M8' }
Ui-Step 'its category'                     { Ui-Key 'alt+g'; Ui-Type 'screws' }
Ui-Step 'Add to Library (Alt+L)'           { Ui-Key 'alt+l' }
Ui-ExpectNew 'Publish: Added screw-m8 to the library, recorded as version ' 'published and recorded' 30
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
Ui-FocusMain
$output = & $cli library check $mine
if ($LASTEXITCODE -ne 0) { Ui-Fail "the published library has errors: $($output -join ' / ')" }
if (-not (Select-String -Path (Join-Path $mine 'mitcad-library.json') -Pattern '"kind": "shaft"' -Quiet)) {
  Ui-Fail 'the published screw lists no shaft frame'
}
Write-Host 'ok   the published screw lists its frame'

Ui-StartApp 'mount-place' @('--open', $scene)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'Library Sources (F8)'             { Ui-Key 'F8' }
Ui-FocusDialog '^Library Sources$'
Add-Source $mine
Ui-Step 'Fetch All (Alt+F)'                { Ui-Key 'alt+f' }
Ui-ExpectNew "Library fetched: library My screws (my-screws) from $mine" 'the own library fetched' 30
Close-Libraries
Ui-Mark
Ui-Step 'Insert from Library (F9)'         { Ui-Key 'F9' }
Ui-FocusDialog '^Browse Components$'
Ui-Step 'search Screw M8'                  { Ui-Key 'alt+s'; Ui-Type 'Screw M8' }
Ui-ExpectNew 'Library browser selected my-screws/screw-m8' 'the own screw'
Ui-Step 'Insert (Alt+I)'                   { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'frames head (into a hole, turns and slides)' "Place Part with the screw's frame"
Ui-ExpectNew 'Command Place Component started' 'Place Part started'
Ui-Mark
# The hole's wall (F2's face), else its rim on the plate's top: whichever
# the view shows (the wall's middle can be out of sight in the hole).
Ui-Step "click the hole's wall or rim"     {
  $hole = Pick-Of '^(face F1\.b0/F2:|edge F1\.b0/E\{.*F1:top.*F2:|edge F1\.b0/E\{.*F2:.*F1:top)'
  if (-not $hole) { Ui-Fail "neither the hole's wall nor its rim among the pick places" }
  Write-Host "note: the hole by $hole"
  Ui-ClickPick "$hole at"
}
Ui-ExpectNew 'Inspect Place Component: place part Screw M8: frame head (shaft), cylindrical joint, on Plate:1' `
  'the screw goes into the hole' 20
Ui-Mark
Ui-Step 'Place (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Placed part Screw M8 (linked) from my-screws ' 'the screw placed' 30
Ui-ExpectNew 'cylindrical joint Joint' 'a cylindrical joint'
Ui-ExpectNew 'at (270, 30, 10)' 'its head on the hole''s rim, coaxial'
$screwBody = (Bodies-Of 'Placed part Screw M8')[0]
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
Save
# The head (14 x 3) on the plate's top, the shank (8 x 20) down through the
# hole: the centre of mass on the hole's axis, 10 - 6.380086 mm up.
Check 'the screw is coaxial with the hole, its head on the plate' @(
  "{`"expect`": {`"occurrence`": `"Screw M8:1`", `"body`": `"$screwBody`", `"center`": [270, 30, 3.619914], `"tolerance`": 1e-3}}")

Write-Host '--- 4. A drop-in joined afterwards with Joint by its mount frame'
# A design of its own: in the scene the plate's top is under the first
# lamp and its sides are a few pixels high in session 0's view, so Joint
# finds none of its faces (only its edges). Here a plate 100 x 60 x 10 at
# (20, 20), its top's middle at (70, 50, 10), in sight.
Ui-StartApp 'mount-join' @('--open', $joinPlate)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Insert 'Lamp' 'lamp'
Ui-Mark
Ui-Step 'drop it in (Enter)'               { Ui-Key 'Return' }
Ui-ExpectNew 'Inserted component Lamp (linked) from test-mounts' 'the lamp dropped in, no choices' 30
Ui-ExpectNew 'mount frames base (face)' 'its frame is a joint origin of its component'
$dropped = (Bodies-Of 'Inserted component Lamp')[0]
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Joint (F11)'                      { Ui-Key 'F11' }
Ui-ExpectNew 'Command Joint started' 'Joint started'
Browser-Reveal 'Browser Root/Lamp:1/Joints/mount:base'
Ui-Step "the lamp's frame in the browser"  { Ui-ClickLogged 'Browser Root/Lamp:1/Joints/mount:base' }
Ui-ExpectNew 'Joint Origin A: 1 ' 'origin A is the mount frame'
Ui-Step "the plate's top"                  { Ui-ClickPick 'face F1.b0/F1:top' }
Ui-Step 'flip'                             { Ui-ClickLogged 'Panel Joint input flip' }
Ui-Mark
Ui-Step 'OK (Enter)'                       { Ui-Key 'Return' }
Ui-ExpectNew 'Added joint Joint' 'joined by its mount frame'
Save $joinPlate
# The lamp's bottom (its frame, flipped onto the face) on the top's middle.
Check 'the dropped-in lamp stands on the plate''s top, by its mount frame' @(
  "{`"expect`": {`"occurrence`": `"Lamp:1`", `"body`": `"$dropped`", `"center`": [70, 50, 12.5], `"tolerance`": 1e-6}}") $joinPlate

Write-Host '--- 5. A hole''s rim selected before Insert from Library: ISO 4762 placed in the hole (mitcad#1197)'
Ui-StartApp 'mount-selected' @('--open', $holed)
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'Library Sources (F8)'             { Ui-Key 'F8' }
Ui-FocusDialog '^Library Sources$'
Add-Source $parts
Ui-Step 'Fetch All (Alt+F)'                { Ui-Key 'alt+f' }
Ui-ExpectNew "Library fetched: library Test ISO parts (test-iso) from $parts" 'the ISO parts fetched' 30
Close-Libraries
# Where a click picks the hole's rim on the plate's top: Fillet logs its
# edges' places (an idle view logs none), cancelled before the click.
Ui-Mark
Ui-Step 'Fillet (Shift+F5)'                { Ui-Key 'shift+F5' }
Ui-ExpectNew 'Command Fillet started' 'Fillet started'
Ui-ExpectNew 'Pick places:' 'where the edges are picked'
$rim = Pick-Of '^(edge F1\.b0/E\{.*F1:top.*F2:|edge F1\.b0/E\{.*F2:.*F1:top)'
if (-not $rim) { Ui-Fail "the hole's rim on the plate's top is not among the pick places" }
Write-Host "note: the rim is $rim"
Ui-Step 'cancel it (Escape)'               { Ui-Key 'Escape' }
Ui-Mark
Ui-Step "select the rim"                   { Ui-ClickPick "$rim at" }
Ui-ExpectNew 'Selected: 1 edge' "the hole's rim selected"
Ui-Mark
Ui-Step 'Insert from Library (F9)'         { Ui-Key 'F9' }
Ui-FocusDialog '^Browse Components$'
Ui-Step 'search 4762'                      { Ui-Key 'alt+s'; Ui-Type '4762' }
Ui-ExpectNew "Library search '4762': 1 components" 'the search finds the screw'
Ui-ExpectNew 'Library browser selected test-iso/iso4762: version ' 'the screw selected'
Ui-Step 'Insert (Alt+I)'                   { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Insert from Library: test-iso/iso4762 has mount frames seat (into a hole, fixed): Place Part' `
  'the screw has its mount frame: Place Component'
Ui-ExpectNew 'Command Place Component started' 'Place Component started'
Ui-ExpectNew 'Place Component: Place On edge' 'the rim selected before is Place On'
Ui-ExpectNew ': frame seat (shaft), rigid joint, on Plate:1' 'the screw shown in the hole at once, no click' 20
if (@(Since-Mark 'Inserted component ').Count -ne 0) { Ui-Fail 'a component was dropped in at the origin' }
Ui-Mark
Ui-Step 'Place (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Placed part ' 'the screw placed' 30
Ui-ExpectNew 'by its frame seat on Plate:1: rigid joint Joint' 'a rigid joint to the hole (logged)'
Ui-ExpectNew 'at (70, 50, 10)' 'its head on the plate''s top, on the hole''s axis'
Ui-Step 'close (Escape)'                   { Ui-Key 'Escape' }
Save $holed
Check 'one screw, fixed in the hole by its joint: none free at the origin' @(
  '{"expect": {"bodies": 2}}',
  '{"expect": {"query": {"query": "joints"}, "result": {"joints": [{"kind": "rigid", "solved": true}]}}}',
  '{"expect": {"query": {"query": "joint_dof", "occurrence": "ISO 4762 M5x16:1"}, "result": {"dof": 0}}}') $holed

Ui-Finish 'UI Windows mount test'
