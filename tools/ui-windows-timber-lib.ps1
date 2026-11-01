# SPDX-License-Identifier: MIT
# What the Windows UI tests of the BUILD tab's TIMBER group share
# (mitcad#834: tools/ui-windows-timber-test.ps1, ui-windows-timber-frames-,
# -wall-, -openings-, -fasteners- and -floor-roof-test.ps1, one ctest each,
# so the test runner runs them side by side; once one script of fifteen app
# sessions, over nine minutes). Dot-sourced right after the test's param()
# (App, Out, QtBin) with $TimberTest set to the test's name; it
#   - initialises the test (tools/ui-windows-lib.ps1's Ui-Init) with the
#     TIMBER group's shortcuts: F3 Profile, F4 Timber, F5 Floor / Roof,
#     F7 Wall, F8 Connector, F9 Cut List, F11 Spacing Set, F12 Timber Frame;
#     the steps without the pause after the sync key (as the Linux tests'
#     steps: Ui-Init -StepPause 0, mitcad#834);
#   - makes a library of small synthetic parts (mitcad-cli; a slot profile,
#     two timber members, one of a less common size, a wood screw and
#     fasteners with heads to click on: a wood screw, a ring nail and a
#     pan-head connector screw in two sizes each) and fetches it from a
#     local git repository ($work, $library; Run-Cli for designs of the
#     test's own); where git is not installed, the test passes skipped;
#   - defines the helpers that read the panels' logs and click the places
#     they log (Timber, Wall, Floor / Roof, Timber Frame, the part picker).

#
# Usage, in a test after its param():
#   $TimberTest = 'UI Windows timber ... test'
#   . (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
#   Timber-Init
# Timber-Init is a function, not code run here: an exit (Ui-Fail, the
# skip) in a dot-sourced file's own code ends only that file, and the test
# would go on.

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $script:cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}

function Timber-Init {
  Ui-Init $App $Out $QtBin @{ 'build.profile' = 'F3'; 'build.timber' = 'F4'; 'build.wall' = 'F7';
                              'build.connector' = 'F8'; 'build.cut_list' = 'F9'; 'build.spacing' = 'F11';
                              'build.floor_roof' = 'F5'; 'build.frame' = 'F12' } -StepPause 0
  # The orientation cube's places logged: the views from a side (a click on
  # the cube).
  $env:MITCAD_LOG_ORIENTATION_CUBE = '1'

  $git = Get-Command git -ErrorAction SilentlyContinue
  if (-not $git) {
    Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
    Write-Host "PASS: $TimberTest (skipped)"
    exit 0
  }
  $script:cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
  if (-not $script:cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
  $script:work = Join-Path $script:UiOut 'files'
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force $work | Out-Null
  $gitConfig = Join-Path $work 'gitconfig'
  Set-Content -Encoding ascii $gitConfig ''
  $env:GIT_CONFIG_GLOBAL = $gitConfig
  $env:GIT_CONFIG_NOSYSTEM = '1'
  $env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
  $script:library = Join-Path $work 'test-timber'

  # Design name script: the commands saved as the library's design <name>.
  function Design([string]$Name, [string]$Script) {
    $json = Join-Path $work (($Name -replace '[\\/]', '-') + '.json')
    Set-Content -Encoding ascii $json $Script
    $target = Join-Path $library $Name
    New-Item -ItemType Directory -Force (Split-Path $target) | Out-Null
    Run-Cli "run $Name" @('run', $json, '--save', $target)
  }

  Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-timber', '--name', 'Test timber', '--license', 'MIT')
  $rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
  function Frame([string]$Name, [string]$Origin, [string]$Z, [string]$X) {
    return ('  {"cmd": "add_feature", "name": "mount:' + $Name + '", "def": {"type": "joint_origin", "geometry": "origin", ' +
            '"frame_override": {"origin": ' + $Origin + ', "z_axis": ' + $Z + ', "x_axis": ' + $X + '}}},')
  }
  # A bar W x T centred on its XY origin, L long along +Z, with its end frames.
  function Bar([double]$W, [double]$T, [string]$Length) {
    return @(
      '[',
      ('  {"cmd": "add_parameter", "name": "L", "expression": "' + $Length + '"},'),
      '  {"cmd": "sketch.create", "plane": "xy"},',
      ('  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-' + ($W / 2) + ', -' + ($T / 2) + '], "width": ' + $W +
       ', "height": ' + $T + '},'),
      ('  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect + '"}], ' +
       '"extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},'),
      (Frame 'end_a' '[0, 0, 0]' '[0, 0, -1]' '[1, 0, 0]'),
      ('  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin", ' +
       '"geometry": {"body": "F2.b0", "face": "F2:end(' + $rect + ')"}}},')) -join "`n"
  }
  # A timber member: the bar with a face frame on each side (docs/libraries.md, "Timber parts").
  function Member([double]$W, [double]$T) {
    return @((Bar $W $T '600 mm'),
      (Frame 'face_xp' ('[' + ($W / 2) + ', 0, 0]') '[1, 0, 0]' '[0, 0, 1]'),
      (Frame 'face_xn' ('[-' + ($W / 2) + ', 0, 0]') '[-1, 0, 0]' '[0, 0, 1]'),
      (Frame 'face_yp' ('[0, ' + ($T / 2) + ', 0]') '[0, 1, 0]' '[0, 0, 1]'),
      (Frame 'face_yn' ('[0, -' + ($T / 2) + ', 0]') '[0, -1, 0]' '[0, 0, 1]')) -join "`n"
  }
  $end = "`n" + '  {"expect": {"bodies": 1}}' + "`n" + ']'
  $profile20 = @((Bar 20 20 '500 mm'), (Frame 'slot_x+_1' '[10, 0, 0]' '[1, 0, 0]' '[0, 0, 1]'),
    (Frame 'slot_x-_1' '[-10, 0, 0]' '[-1, 0, 0]' '[0, 0, 1]'), (Frame 'slot_y+_1' '[0, 10, 0]' '[0, 1, 0]' '[0, 0, 1]'),
    (Frame 'slot_y-_1' '[0, -10, 0]' '[0, -1, 0]' '[0, 0, 1]')) -join "`n"
  Design 'profiles\p20-test.mitcad' ($profile20 + $end)
  # The 48 member in three sizes (mitcad#484): W x T from its rows, centred on
  # XY, its face frames on planes at W / 2 and T / 2 (the generator's layout).
  function Side-Frame([string]$Name, [string]$Normal, [string]$Distance, [int]$Plane) {
    return ('  {"cmd": "add_feature", "name": "Plane of ' + $Name + '", "def": {"type": "construction_plane", ' +
            '"definition": {"type": "offset", "plane": {"origin": [0, 0, 0], "normal": ' + $Normal +
            ', "x_axis": [0, 0, 1]}, "distance": "' + $Distance + '"}}},' + "`n" +
            '  {"cmd": "add_feature", "name": "mount:' + $Name + '", "def": {"type": "joint_origin", "geometry": "F' +
            $Plane + '"}},')
  }
  $sizes = @(@('48x98', 98), @('48x148', 148), @('48x198', 198)) | ForEach-Object {
    '    {"name": "' + $_[0] + '", "select": {"Size": "' + $_[0] + '"}, "values": {"W": "' + $_[1] + ' mm", "T": "48 mm"}}' }
  $member48 = @('[',
    '  {"cmd": "add_parameter", "name": "L", "expression": "600 mm"},',
    '  {"cmd": "add_parameter", "name": "W", "expression": "98 mm"},',
    '  {"cmd": "add_parameter", "name": "T", "expression": "48 mm"},',
    ('  {"cmd": "add_feature", "name": "Section", "def": {"type": "sketch", "plane": "xy", "entities": [' +
     '{"id": "p1", "type": "point", "at": [-49, -24]}, {"id": "p2", "type": "point", "at": [49, -24]}, ' +
     '{"id": "p3", "type": "point", "at": [49, 24]}, {"id": "p4", "type": "point", "at": [-49, 24]}, ' +
     '{"id": "p5", "type": "point", "at": [0, 0], "fixed": true}, ' +
     '{"id": "c6", "type": "line", "start": "p1", "end": "p2"}, {"id": "c7", "type": "line", "start": "p2", "end": "p3"}, ' +
     '{"id": "c8", "type": "line", "start": "p3", "end": "p4"}, {"id": "c9", "type": "line", "start": "p4", "end": "p1"}, ' +
     '{"id": "c10", "type": "line", "start": "p1", "end": "p3", "construction": true}], "constraints": [' +
     '{"id": "k11", "type": "horizontal", "line": "c6"}, {"id": "k12", "type": "vertical", "line": "c7"}, ' +
     '{"id": "k13", "type": "horizontal", "line": "c8"}, {"id": "k14", "type": "vertical", "line": "c9"}, ' +
     '{"id": "k15", "type": "midpoint", "point": "p5", "curve": "c10"}], "dimensions": [' +
     '{"id": "k16", "type": "length", "line": "c6", "value": "W"}, {"id": "k17", "type": "length", "line": "c7", "value": "T"}]}},'),
    ('  {"cmd": "add_feature", "name": "Profile", "def": {"type": "extrude", "profiles": [{"sketch": "F1", ' +
     '"region": "r{c6[c9,c7],c7[c6,c8],c8[c7,c9],c9[c8,c6]}"}], "extent": {"type": "distance", "distance": "L"}, ' +
     '"operation": "new_body"}},'),
    ('  {"cmd": "add_feature", "name": "End B plane", "def": {"type": "construction_plane", ' +
     '"definition": {"type": "offset", "plane": "xy", "distance": "L"}}},'),
    '  {"cmd": "add_feature", "name": "mount:end_a", "def": {"type": "joint_origin", "geometry": "xy", "flip": true}},',
    '  {"cmd": "add_feature", "name": "mount:end_b", "def": {"type": "joint_origin", "geometry": "F3"}},',
    (Side-Frame 'face_xp' '[1, 0, 0]' 'W / 2' 6), (Side-Frame 'face_xn' '[-1, 0, 0]' 'W / 2' 8),
    (Side-Frame 'face_yp' '[0, 1, 0]' 'T / 2' 10), (Side-Frame 'face_yn' '[0, -1, 0]' 'T / 2' 12),
    ('  {"cmd": "set_configurations", "configurations": {"selectors": ["Size"], "parameters": ["W", "T"], ' +
     '"default": "48x98", "rows": [' + "`n" + ($sizes -join ",`n") + ']}},'),
    '  {"expect": {"total_volume": 2822400, "tolerance": 1e-6}},') -join "`n"
  Design 'dimensioned-timber\timber-48-test.mitcad' ($member48 + $end)
  Design 'planed-timber\timber-28-test.mitcad' ((Member 120 28) + $end)
  # Pictures: the 48 member's (16 px) and its row 48x148's own (24 px).
  Add-Type -AssemblyName System.Drawing
  New-Item -ItemType Directory -Force (Join-Path $library 'previews') | Out-Null
  foreach ($picture in @(@('timber-48-test', 16), @('timber-48-test-48x148', 24))) {
    $bitmap = New-Object System.Drawing.Bitmap $picture[1], $picture[1]
    $bitmap.Save((Join-Path $library "previews\$($picture[0]).png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $bitmap.Dispose()
  }
  # A wood screw: a 5 x 5 shank 70 long along +Z, its axis frame at the head.
  $screw = @('[',
    '  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0], "length": 5, "width": 5, "height": 70, "operation": "new_body"}},',
    (Frame 'axis' '[2.5, 2.5, 0]' '[0, 0, 1]' '[1, 0, 0]')) -join "`n"
  Design 'fasteners\wood-screw-test.mitcad' ($screw + $end)
  # Fasteners in two sizes (mitcad#497): a head SIZE x SIZE x 10 above the
  # axis frame (the entry point), outside the members to click on.
  function Head-Fastener([int]$Size, [string]$Row1, [string]$Row2) {
    return @('[',
      ('  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": [-' + ($Size / 2) +
       ', -' + ($Size / 2) + '], "length": ' + $Size + ', "width": ' + $Size + ', "height": 10, "operation": "new_body"}},'),
      (Frame 'axis' '[0, 0, 10]' '[0, 0, 1]' '[1, 0, 0]'),
      ('  {"cmd": "set_configurations", "configurations": {"parameters": [], "default": "' + $Row1 + '", "rows": [{"name": "' +
       $Row1 + '"}, {"name": "' + $Row2 + '"}]}},')) -join "`n"
  }
  Design 'fasteners\wood-screw-head-test.mitcad' ((Head-Fastener 30 '5.0x70' '5.0x90') + $end)
  Design 'fasteners\ring-nail-test.mitcad' ((Head-Fastener 20 '2.8x50' '3.1x90') + $end)
  # A pan-head connector screw (mitcad#516): its name comes first, but it
  # bears on a plate and is no frame's default.
  Design 'fasteners\connector-screw-test.mitcad' ((Head-Fastener 24 '5.0x40' '5.0x60') + $end)

  Set-Content -Encoding ascii (Join-Path $library 'mitcad-library.json') @'
{
  "format": "mitcad-library", "version": 1, "id": "test-timber", "name": "Test timber",
  "description": "Synthetic BUILD parts of the timber UI test.", "library_version": "1.0.0", "license": "MIT",
  "authors": ["Mitcad UI test"], "units": "mm",
  "categories": [{"id": "profiles", "name": "Profiles"}, {"id": "dimensioned-timber", "name": "Dimensioned timber"},
                 {"id": "planed-timber", "name": "Planed timber"}, {"id": "fasteners", "name": "Fasteners"}],
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
    {"id": "timber-28-test", "path": "planed-timber/timber-28-test.mitcad", "name": "Timber 28x120 test",
     "category": "planed-timber", "designation": "Timber 28x120 test L={L}",
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "pine", "finish": "planed", "grade": "", "density": 520, "moisture": 20},
       "availability": "less common",
       "section": {"width": "120 mm", "height": "28 mm"}, "nominal": {"width": "120 mm", "height": "28 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "600 mm", "stock": ["3000 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}]}},
    {"id": "timber-48-test", "path": "dimensioned-timber/timber-48-test.mitcad", "name": "Timber 48x98 test",
     "category": "dimensioned-timber", "designation": "Timber 48x98 test L={L}",
     "preview": "previews/timber-48-test.png", "row_previews": {"48x148": "previews/timber-48-test-48x148.png"},
     "build": {"role": "profile", "kind": "timber",
       "material": {"species": "spruce", "finish": "dimensioned", "grade": "", "density": 450, "moisture": 20},
       "availability": "standard",
       "section": {"width": "98 mm", "height": "48 mm"}, "nominal": {"width": "98 mm", "height": "48 mm"},
       "length": {"parameter": "L", "min": "100 mm", "max": "6000 mm", "default": "3000 mm",
                  "stock": ["3000 mm", "3600 mm"]},
       "frames": [{"name": "end_a", "kind": "end"}, {"name": "end_b", "kind": "end"},
                  {"name": "face_xp", "kind": "face"}, {"name": "face_xn", "kind": "face"},
                  {"name": "face_yp", "kind": "face"}, {"name": "face_yn", "kind": "face"}],
       "rows": {"48x148": {"section": {"width": "148 mm", "height": "48 mm"},
                           "nominal": {"width": "148 mm", "height": "48 mm"}},
                "48x198": {"section": {"width": "198 mm", "height": "48 mm"},
                           "nominal": {"width": "198 mm", "height": "48 mm"}, "availability": "less common"}}}},
    {"id": "connector-screw-test", "path": "fasteners/connector-screw-test.mitcad", "name": "Connector screw test",
     "category": "fasteners", "designation": "Connector screw test {config}",
     "build": {"role": "fastener", "wood": {"kind": "screw", "head": "pan", "seat": "head_underside",
                                            "diameter": "5 mm", "length": "40 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}],
       "rows": {"5.0x40": {"wood": {"diameter": "5 mm", "length": "40 mm"}},
                "5.0x60": {"wood": {"diameter": "5 mm", "length": "60 mm"}}}}},
    {"id": "wood-screw-test", "path": "fasteners/wood-screw-test.mitcad", "name": "Wood screw test",
     "category": "fasteners", "designation": "Wood screw test 5x70",
     "build": {"role": "fastener", "wood": {"kind": "screw", "head": "countersunk", "diameter": "5 mm", "length": "70 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}]}},
    {"id": "wood-screw-head-test", "path": "fasteners/wood-screw-head-test.mitcad", "name": "Wood screw head test",
     "category": "fasteners", "designation": "Wood screw head test {config}",
     "build": {"role": "fastener", "wood": {"kind": "screw", "head": "countersunk", "diameter": "5 mm", "length": "70 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}],
       "rows": {"5.0x70": {"wood": {"diameter": "5 mm", "length": "70 mm"}},
                "5.0x90": {"wood": {"diameter": "5 mm", "length": "90 mm"}}}}},
    {"id": "ring-nail-test", "path": "fasteners/ring-nail-test.mitcad", "name": "Ring nail test",
     "category": "fasteners", "designation": "Ring nail test {config}",
     "build": {"role": "fastener", "wood": {"kind": "nail", "diameter": "3.1 mm", "length": "90 mm"},
       "material": {"product": "steel", "density": 7850},
       "frames": [{"name": "axis", "kind": "shaft"}],
       "rows": {"2.8x50": {"wood": {"diameter": "2.8 mm", "length": "50 mm"}},
                "3.1x90": {"wood": {"diameter": "3.1 mm", "length": "90 mm"}}}}}
  ]
}
'@
  foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'Test parts'), @('tag', 'v1.0.0'))) {
    $output = & $git.Source -C $library -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
    if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
  }
  Run-Cli 'library fetch' @('library', 'fetch', $library)
}

function Placed-Body { return Ui-LastMatch '^New body .* \((F[0-9]+\.b[0-9]+)\): volume' }
# The faces of a body a click can pick now (after the last "Pick places:")
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
function Pick-At([string]$Body, [string]$Face) { return Ui-PickAt "$Body/$Face at" }
# The view fitted to the parts, and the places a click picks logged again.
function Refresh-Picks([string]$Field) {
  Ui-Step 'fit the view (F6)'            { Ui-Key 'F6' }
  Ui-Step "Timber's $Field again"        { Ui-ClickLogged "Panel Timber input $Field" }
}
# Picker-Click TEXT: a click on a place of the part picker that it logged
# ("Part picker tile <part> at x,y; in the picker x,y"), posted to the
# picker's own window (tools/ui-windows-build-test.ps1).
function Picker-Click([string]$Text) {
  $place = $null
  for ($i = 0; $i -lt 120 -and -not $place; $i++) {
    $line = Ui-LastLineWith "$Text at "
    $match = [regex]::Match("$line", 'in the picker (-?\d+),(-?\d+)$')
    if ($match.Success) { $place = @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value) }
    else { Start-Sleep -Milliseconds 50 }
  }
  if (-not $place) { Ui-Fail "no place of the part picker logged for '$Text'" }
  $picker = Ui-Windows '^Component Picker'
  if ($picker.Count -eq 0) { Ui-Fail 'no part picker window' }
  $b = $script:UiButtons[1]
  Ui-Apart $picker[0] $place[0] $place[1]
  [UiInput]::Move($picker[0], $place[0], $place[1], 0)
  Start-Sleep -Milliseconds 80
  [UiInput]::Button($picker[0], $b[0], $place[0], $place[1], $b[3])
  Start-Sleep -Milliseconds 60
  [UiInput]::Button($picker[0], $b[1], $place[0], $place[1], 0)
}
# Picker-Type TEXT: typed into the part picker's window (its focused field).
function Picker-Type([string]$Text) {
  $picker = Ui-Windows '^Component Picker'
  if ($picker.Count -eq 0) { Ui-Fail 'no part picker window' }
  [UiInput]::Type($picker[0], $Text, 40)
}
# Picker-Keys @(VK, ...) COUNT: each virtual key COUNT times, to the part
# picker's window (End, BackSpace in its search field).
function Picker-Keys([int[]]$Keys, [int]$Count) {
  $picker = Ui-Windows '^Component Picker'
  if ($picker.Count -eq 0) { Ui-Fail 'no part picker window' }
  foreach ($vk in $Keys) {
    for ($i = 0; $i -lt $Count; $i++) {
      [UiInput]::Key($picker[0], $vk, $false, $false, $false)
      Start-Sleep -Milliseconds 60
    }
  }
}
function Place([string]$What) {
  Ui-Mark
  Ui-Step 'Place (Enter)'                { Ui-Key 'Return' }
  Ui-ExpectNew "Placed $What as " "$What placed" 30
}
$side = ':part(F2:side('
# Set-Previewed TEXT description: a spacing set's preview after the click
# (mitcad#487) in an "Inspect Timber:" line since the mark with TEXT: as
# many bodies drawn as the model lays out members, as many as the panel
# counts, two or more; returns the count.
function Set-Previewed([string]$Text, [string]$Description) {
  $pattern = '; set preview (\d+) member\(s\), (\d+) body\(ies\).*; set of (\d+),'
  $line = $null
  for ($i = 0; $i -lt 200 -and -not $line; $i++) {
    $line = Ui-NewLines |
      Where-Object { $_.StartsWith('Inspect Timber: ') -and $_.Contains($Text) -and $_ -match $pattern } |
      Select-Object -Last 1
    if (-not $line) { Start-Sleep -Milliseconds 50 }
  }
  if (-not $line) { Ui-Fail "${Description}: no set preview in an 'Inspect Timber:' line with '$Text' since the mark" }
  $null = "$line" -match $pattern
  $members = [int]$Matches[1]
  $bodies = [int]$Matches[2]
  $counted = [int]$Matches[3]
  if ($members -lt 2 -or $bodies -ne $members -or $counted -ne $members) {
    Ui-Fail "${Description}: the preview shows $bodies body(ies) of $members member(s), the panel counts ${counted}: $line"
  }
  Write-Host "ok   ${Description}: all $members members previewed"
  return $members
}

# Refresh-At CMD: the view fitted to the parts and CMD's Place On picked
# again: the places a click picks logged again.
function Refresh-At([string]$Cmd) {
  Ui-Step 'fit the view (F6)'            { Ui-Key 'F6' }
  Ui-Step "$Cmd's Place On again"        { Ui-ClickLogged "Panel $Cmd input at" }
}
# Expect-Inspect CMD TEXTS description: an "Inspect CMD:" line since the
# mark has every text.
function Expect-Inspect([string]$Cmd, [string[]]$Texts, [string]$Description) {
  for ($i = 0; $i -lt 120; $i++) {
    $found = Ui-NewLines | Where-Object { $_.StartsWith("Inspect ${Cmd}: ") } |
      Where-Object { $line = $_; @($Texts | Where-Object { -not $line.Contains($_) }).Count -eq 0 } |
      Select-Object -First 1
    if ($found) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 50
  }
  Ui-Fail "${Description}: no 'Inspect ${Cmd}:' line since the mark with $($Texts -join ' / ')"
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
# No-Ghost CMD TEXT description: a moment after Place, no "Inspect CMD:
# TEXT" line since the mark: nothing previewed at the default place on top
# of what was just placed (mitcad#513).
function No-Ghost([string]$Cmd, [string]$Text, [string]$Description) {
  Start-Sleep -Milliseconds 1500
  Ui-Sync
  $found = @(Ui-NewLines | Where-Object { $_.StartsWith("Inspect ${Cmd}: $Text") })
  if ($found.Count -gt 0) { Ui-Fail "${Description}: $($found[0])" }
  Write-Host "ok   $Description"
}
# Last-EndA CMD: where the last "Inspect CMD:" line puts the preview's end
# A ("; end A at x, y, z").
function Last-EndA([string]$Cmd) {
  $line = Ui-LastLineWith "Inspect ${Cmd}: " -Start
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
  for ($i = 0; $i -lt 120 -and $places.Count -eq 0; $i++) {
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
# Hover-Names CMD BODY FACE AT: before a click, the mouse over BODY's FACE
# at AT (its pick place) names the face a click picks and previews
# nothing: no "Inspect CMD:" line (mitcad#489, mitcad#508).
function Hover-Names([string]$Cmd, [string]$Body, [string]$Face, [int[]]$At) {
  Ui-Sync
  Start-Sleep -Milliseconds 500
  Ui-Mark
  # MK_SHIFT keeps the move in session 0 (tools/ui-windows-build-test.ps1).
  Ui-Step "the mouse over $Body's $Face"  { [UiInput]::Move($script:UiWindow, $At[0], $At[1], 0x04) }
  Ui-ExpectNew "BUILD hover pick: face $Face of $Body" 'the hover names the face a click picks'
  Ui-Step 'the mouse a little further'   { [UiInput]::Move($script:UiWindow, ($At[0] + 6), $At[1], 0x04) }
  No-Inspect $Cmd 'no preview and no inspection while hovering before the click'
}
# Keeps-Place CMD AT OTHER_BODY OTHER_FACE: after a click at AT (a face's
# pick place), the preview stays there while the mouse moves along the
# face, off the parts and back: no "Inspect CMD:" line, its end A
# unchanged, the hover still logged; a click on OTHER_BODY's OTHER_FACE
# moves it once, a click at AT again puts it back (mitcad#508).
function Keeps-Place([string]$Cmd, [int[]]$At, [string]$OtherBody, [string]$OtherFace) {
  Ui-Sync
  $endA = Last-EndA $Cmd
  if (-not $endA) { Ui-Fail "no end A in the inspection of the click" }
  Ui-Mark
  Ui-Step 'the mouse along the face'     { [UiInput]::Move($script:UiWindow, ($At[0] + 12), $At[1], 0x04) }
  Ui-Step 'the mouse off the parts'      { Off-Parts (View-Area) }
  Ui-ExpectNew "BUILD hover pick: off the parts' faces" 'the hover off the parts'
  Ui-Step 'the mouse on the face again'  { [UiInput]::Move($script:UiWindow, $At[0], $At[1], 0x04) }
  No-Inspect $Cmd 'no preview and no inspection while the mouse moves after the click'
  if ((Last-EndA $Cmd) -ne $endA) { Ui-Fail "the preview's end A moved from $endA" }
  Write-Host "ok   the preview's end A still at $endA"
  Ui-Mark
  Ui-Step "a click on $OtherBody's $OtherFace" { Pick-Face $OtherBody $OtherFace }
  Moved-Once $Cmd $endA 'a click on another face moves the preview once'
  Ui-Mark
  Ui-Step 'a click at the first place again' { Ui-Click $At[0] $At[1] }
  Ui-ExpectNew "Inspect ${Cmd}: " 'previewed at the first click again'
  Ui-Sync
  if ((Last-EndA $Cmd) -ne $endA) { Ui-Fail "the click at the first place again: end A not at $endA" }
  Write-Host "ok   the click at the first place again: end A at $endA"
}
# Marker-At CMD LABEL: where CMD's last preview marker LABEL (a regex; also
# the taken one, "> LABEL") is; its point is on the face picked, so a click
# there clicks that face (tools/ui-windows-build-test.ps1).
function Marker-At([string]$Cmd, [string]$Label) {
  $line = Ui-LogLines | Where-Object { $_ -match "^Preview marker $Cmd (> )?$Label world " } | Select-Object -Last 1
  if ("$line" -notmatch ' at (\d+),(\d+)$') { Ui-Fail "no preview marker '$Label' of $Cmd logged" }
  return @([int]$Matches[1], [int]$Matches[2])
}
# Off-Parts AREA: the mouse off the parts, to the view's top left corner
# (AREA: x y width height of the view; tools/ui-windows-build-test.ps1).
function Off-Parts([int[]]$Area) {
  [UiInput]::Move($script:UiWindow, ($Area[0] + 6), ($Area[1] + 6), 0x04)
}
function View-Area {
  $area = @((Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)') -split ' ' | ForEach-Object { [int]$_ })
  if ($area.Count -lt 4) { Ui-Fail 'the app did not log its view area' }
  return , $area
}
# Pick-Markers CMD AREA: the mouse off the parts (Off-Parts) and the pick's
# markers logged (the hover previews nothing, mitcad#508).
function Pick-Markers([string]$Cmd, [int[]]$Area) {
  Off-Parts $Area
  Ui-Sync
  $lines = @(Ui-LogLines)
  $inspect = 0
  for ($j = $lines.Count - 1; $j -ge 0; $j--) { if ($lines[$j].StartsWith("Inspect ${Cmd}: ")) { $inspect = $j; break } }
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    $lines = @(Ui-LogLines)
    for ($j = $inspect; $j -lt $lines.Count; $j++) { if ($lines[$j].StartsWith("Preview marker $Cmd ")) { return } }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 2500)
}
# Marker-InMiddle CMD LABEL: where CMD's marker LABEL is once it lies in the
# middle half of the view, the view zoomed step by step at its centre until
# then (tools/ui-windows-build-test.ps1).
function Marker-InMiddle([string]$Cmd, [string]$Label) {
  $area = @((Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)') -split ' ' | ForEach-Object { [int]$_ })
  if ($area.Count -lt 4) { Ui-Fail 'the app did not log its view area' }
  $cx = [int]($area[0] + $area[2] / 2)
  $cy = [int]($area[1] + $area[3] / 2)
  $sign = -1
  $last = $null
  for ($i = 0; $i -lt 16; $i++) {
    Pick-Markers $Cmd $area
    $at = Marker-At $Cmd $Label
    $off = [Math]::Max([Math]::Abs($at[0] - $cx) / $area[2], [Math]::Abs($at[1] - $cy) / $area[3])
    if ($off -lt 0.25) { return $at }
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
  Ui-Fail "${Cmd}'s marker '$Label' did not come into the middle of the view"
}
# Wheel-Choice CMD INPUT STEPS: a panel drop-down moved by STEPS entries
# (positive: down the list), one wheel step at a time.
function Wheel-Choice([string]$Cmd, [string]$Choice, [int]$Steps) {
  for ($i = 0; $i -lt [Math]::Abs($Steps); $i++) {
    Ui-WheelLogged "Panel $Cmd input $Choice" ([Math]::Sign(-$Steps))
    # The step handled and the panel's places logged again (mitcad#870: a
    # fixed 150 ms before).
    Ui-Sync
  }
}
# Corner CMD BODY TURN [LENGTH]: a part on BODY's end B turned towards TURN
# (a host axis: "-Y") by a click on the end face's band along that edge (its
# zone's marker), with a shared mitre, LENGTH mm long (none: the auto length
# to the face, mitcad#485).
function Corner([string]$Cmd, [string]$Body, [string]$Turn, [string]$Length) {
  Refresh-At $Cmd
  $ends = @(Faces-Of $Body ':end_b')
  if ($ends.Count -eq 0) { Ui-Fail "no end B of $Body to pick" }
  Ui-Mark
  Ui-Step "$Body's end B, in the middle"  { Pick-Face $Body $ends[0] }
  Ui-ExpectNew 'BUILD end click: straight towards ' 'the end picked, straight on'
  $at = Marker-InMiddle $Cmd ('turn ' + [regex]::Escape($Turn))
  Ui-Mark
  Ui-Step "near its edge towards $Turn"  { Ui-Click $at[0] $at[1] }
  Ui-ExpectNew "BUILD end click: edge towards $Turn" "turned towards $Turn"
  # The panel keeps the mitre chosen for the corner before.
  $line = $null
  for ($i = 0; $i -lt 120 -and -not $line; $i++) {
    $line = Ui-NewLines |
      Where-Object { $_.StartsWith("Inspect ${Cmd}: ") -and $_.Contains("; end edge towards $Turn") } | Select-Object -Last 1
    if (-not $line) { Start-Sleep -Milliseconds 50 }
  }
  if (-not "$line".Contains('mitre shared')) {
    Ui-Mark
    Ui-Step 'Mitre: Shared'              { Wheel-Choice $Cmd 'mitre' 2 }
  }
  Expect-Inspect $Cmd @("; end edge towards $Turn", 'mitre shared, offset 0.0 mm') 'a shared mitre at the corner'
  Ui-Mark
  if ($Length) {
    Ui-Step "$Length mm long"            { Ui-TypeIn "Panel $Cmd input length" $Length }
    Expect-Inspect $Cmd @("length $Length.00 mm (typed)", "; end edge towards $Turn", 'mitre shared') "$Length mm, the corner mitred"
  } else {
    Ui-Step 'Auto: to the face'          { Ui-ClickLogged "Panel $Cmd input length_mode=face" }
  }
}

# Wall-Sides BODIES: the side faces of these bodies a click on Start picks
# now (after the last "Pick places:"), each @(body, face, x, y).
function Wall-Sides([string[]]$Bodies) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  return @($lines[$start..($lines.Count - 1)] | ForEach-Object {
      if ($_ -match '^Pick face (F[0-9]+\.b[0-9]+)/(\S*:part\(F2:side\(\S*) at (\d+),(\d+)$' -and $Bodies -contains $Matches[1]) {
        , @($Matches[1], $Matches[2], [int]$Matches[3], [int]$Matches[4])
      } })
}
# New-Bodies: the bodies made since the mark.
function New-Bodies {
  return @(Ui-NewLines |
    ForEach-Object { if ($_ -match '^New body .* \((F[0-9]+\.b[0-9]+)\): volume') { $Matches[1] } })
}
# Wall-Ends BODIES: in the view from the right (+X), the faces of these
# bodies a click on Start picks now: their end B's face, the plates' ends
# (end_b) first, then the end stud's side; each @(body, face, x, y)
# (mitcad#510). The plates' ends are too small to pick in the fitted view
# from the front right.
function Wall-Ends([string[]]$Bodies) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $faces = @($lines[$start..($lines.Count - 1)] | ForEach-Object {
      if ($_ -match '^Pick face (F[0-9]+\.b[0-9]+)/(\S*) at (\d+),(\d+)$' -and $Bodies -contains $Matches[1]) {
        , @($Matches[1], $Matches[2], [int]$Matches[3], [int]$Matches[4])
      } })
  return @(@($faces | Where-Object { $_[1].Contains(':end_b') }) + @($faces | Where-Object { -not $_[1].Contains(':end_b') }))
}
# Wall-From NAME AT LENGTH TEXT STUDS [CORNER]: a wall started by a click
# at AT (x, y) on Wall 1, LENGTH mm long (typed): the panel names where it
# starts (TEXT) and the Corner chosen (CORNER, default butt), the wall is
# placed with as many members as previewed, and Wall 1 gets at least STUDS
# more (a T's studs may take a stud's place).
function Wall-From([string]$Name, [int[]]$At, [string]$Length, [string]$Text, [int]$Studs,
                   [string]$Corner = 'butt') {
  Ui-Mark
  Ui-Step "the start on Wall 1"          { Ui-Click $At[0] $At[1] }
  Ui-ExpectNew 'View hint Wall: Click where the wall ends, or press Enter: Length square to the wall clicked' `
    'the end next, else Length square to the wall'
  Ui-Mark
  Ui-Step "$Length mm long"              { Ui-TypeIn 'Panel Wall input frame_length' $Length }
  Expect-Inspect 'Wall' @("frame wall $Name, wall $Length ${Times} 2400 mm", "; $Name`: $Text", "; corner $Corner") `
    "$Name previewed: $Text, corner $Corner"
  $line = Ui-NewLines |
    Where-Object { $_.StartsWith('Inspect Wall: ') -and $_.Contains("; $Name`: $Text") } | Select-Object -Last 1
  if ("$line" -notmatch ', (\d+) member\(s\) \(') { Ui-Fail "no member count in: $line" }
  $previewed = [int]$Matches[1]
  Ui-Mark
  Ui-Step 'Place (Enter)'                { Ui-Key 'Return' }
  Ui-ExpectNew "Placed frame $Name (wall, lead " "$Name placed" 60
  Ui-ExpectNew "Frame junction: $Name`: $Text" "$Name placed: $Text"
  $placed = [int](Ui-LastMatch "^Placed frame $Name \(wall, lead [^)]*\): (\d+) member")
  if ($placed -ne $previewed) { Ui-Fail "$Name placed $placed member(s), the preview showed $previewed" }
  $made = @(New-Bodies).Count
  if ($made -lt $placed + $Studs) {
    Ui-Fail "$($made) new bodies after $Name, not its $placed members and at least $Studs junction stud(s) of Wall 1"
  }
  Write-Host "ok   $Name`: $placed members, Wall 1 $($made - $placed) body(ies) more"
}

# First-Wall DESIGN CORNER: in a new design, Wall (mitcad#520) without
# fasteners, Corner CORNER (butt, the default, or lapped: chosen in the
# panel), Wall 1 placed from the origin along +X; its bodies in $Wall1.
function First-Wall([string]$Design, [string]$Corner) {
  Ui-StartApp $Design
  Ui-Step 'the BUILD tab'                { Ui-ClickLogged 'Ribbon tab BUILD' }
  Ui-ExpectLog 'Ribbon BUILD/TIMBER at' 'its TIMBER group (mitcad#520)'
  Ui-ExpectLog 'Ribbon button build.wall at' 'the Wall button'
  Ui-Mark
  Ui-Step 'Wall (F7)'                    { Ui-Key 'F7' }
  Ui-ExpectNew 'Command Wall started' 'Wall started'
  Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm", 'fasteners yes', '; corner butt') `
    'a wall from the origin along +X, Corner Butt the first time'
  Ui-Mark
  Ui-Step 'no fasteners'                 { Ui-ClickLogged 'Panel Wall input frame_fasteners' }
  Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm", 'fasteners no') 'without fasteners'
  if ($Corner -eq 'lapped') {
    Ui-Mark
    Ui-Step 'Corner: Lapped plates'      { Ui-ClickLogged 'Panel Wall input frame_corner=lapped' }
    Expect-Inspect 'Wall' @("frame wall Wall 1, wall 3600 ${Times} 2400 mm", '; corner lapped') 'Corner Lapped plates'
  }
  Ui-Mark
  Ui-Step 'Place (Enter)'                { Ui-Key 'Return' }
  Ui-ExpectNew 'Placed frame Wall 1 (wall, lead ' 'the first wall placed' 60
  $script:Wall1 = @(New-Bodies)
  $script:Studs1 = @(Stud-Bodies)
  if ($script:Wall1.Count -lt 4) { Ui-Fail "Wall 1 made $($script:Wall1.Count) bodies" }
}
# Stud-Bodies: the 2304 mm studs (98 x 48: 10838016 mm3) made since the
# mark (mitcad#514).
function Stud-Bodies {
  return @(Ui-NewLines | ForEach-Object {
      if ($_ -match '^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$' -and
          [Math]::Abs([double]$Matches[2] - 10838016) -lt 1) { $Matches[1] } } | Select-Object -Unique)
}
# Stud-Face BODIES WHICH: @(x, y) of a side face of these studs a click on
# Start picks now (Wall-Sides): WHICH 'first' the leftmost face of all (the
# leftmost stud's front face), 'last' the leftmost face of the rightmost
# stud (its front face, not its end's), 'middle' the rightmost face of the
# middle stud; the studs ordered by their rightmost face (the view from
# the front right: +X and +Y to the right, a stud's +X face right of its
# -Y face).
function Stud-Face([string[]]$Bodies, [string]$Which) {
  $faces = @(Wall-Sides $Bodies)
  if ($faces.Count -lt 1) { Ui-Fail 'no side face of the studs to click' }
  $studs = @($faces | Group-Object { $_[0] } | ForEach-Object {
      $sorted = @($_.Group | Sort-Object { $_[2] })
      [pscustomobject]@{ Lo = $sorted[0]; Hi = $sorted[-1] }
    } | Sort-Object { $_.Hi[2] })
  switch ($Which) {
    'first' { $face = $studs[0].Lo }
    'last' { $face = $studs[-1].Lo }
    default { $face = $studs[[int][Math]::Floor(($studs.Count - 1) / 2)].Hi }
  }
  return @($face[2], $face[3])
}
# Start-Picks: the view fitted, the places a click on Start picks logged.
function Start-Picks {
  Ui-Step 'fit the view (F6)'            { Ui-Key 'F6' }
  Ui-Step "Wall's Start"               { Ui-ClickLogged 'Panel Wall input start' }
}

# Heads-Since VOLUME: the fasteners' head bodies made since the mark (a
# head's volume: 9000 the screw's, 4000 the nail's).
function Heads-Since([string]$Volume) {
  return @(Ui-NewLines |
    ForEach-Object { if ($_ -match "^New body .* \((F[0-9]+\.b[0-9]+)\): volume $Volume\b") { $Matches[1] } })
}
# Choose-Fastener CMD INPUT PART: the input's tile opens the part picker, a
# click on the part's tile chooses it.
function Choose-Fastener([string]$Cmd, [string]$Field, [string]$Part) {
  Ui-Mark
  Ui-Step "the $Field tile"              { Ui-ClickLogged "Panel $Cmd input $Field" }
  Ui-ExpectNew 'Part picker opened: ' "the part picker for $Cmd's $Field"
  Ui-ExpectNew "Part picker tile $Part at " "$Part's tile in the picker"
  Ui-Mark
  Ui-Step "$Part in the picker"          { Picker-Click "Part picker tile $Part" }
  Ui-ExpectNew "Part picker chose $Part" "$Part chosen in the picker"
  # The picker stays open beside the panel (input is posted to the main
  # window, which it does not cover) and closes with the command.
}

# Largest-Bodies: the bodies of the largest volume made since the mark (a
# wall's plates).
function Largest-Bodies {
  $made = @(Ui-NewLines | ForEach-Object {
      if ($_ -match '^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$') {
        [pscustomobject]@{ Body = $Matches[1]; Volume = [double]$Matches[2] } } })
  if ($made.Count -eq 0) { return @() }
  $most = ($made | Measure-Object -Property Volume -Maximum).Maximum
  return @($made | Where-Object { $_.Volume -eq $most } | ForEach-Object { $_.Body })
}
# Pickable BODIES: the first of these bodies a click picks a face of now (a
# "Pick face" place after the last "Pick places:"); $null for none.
function Pickable([string[]]$Bodies) {
  $lines = @(Ui-LogLines)
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $picks = @($lines[$start..($lines.Count - 1)])
  foreach ($body in $Bodies) {
    if (@($picks | Where-Object { $_.StartsWith("Pick face $body/") }).Count -gt 0) { return $body }
  }
  return $null
}
