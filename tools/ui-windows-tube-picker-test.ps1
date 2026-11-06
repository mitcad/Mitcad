# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library tools/cli
# check-all sources: tools/libraries/make-profile-library.py tools/libraries/library_previews.py tools/libraries/library_tags.py tools/libraries/profiles/steel-tubes.json
# Input-driven Windows UI test of the ASTM A500 tubes in BUILD's part
# picker (mitcad#1372, GitHub mitcad/Mitcad#159: a search for A500 found
# nothing in a library without them) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted); the Linux test
# is tools/ui-tube-picker-test.sh, with the same steps. The parts library's
# square and rectangular A500 tubes, made here by
# tools/libraries/make-profile-library.py from tools/libraries/profiles
# (--only, as the parts library makes them since 2.11.0), fetched from a
# local git repository; skipped where git is not installed:
#   1. Profile, the part picker from the panel's Component tile: every row
#      of both tubes a tile.
#   2. The search A500 lists them all.
#   3. A500 3x3x3/16: the square tube HSS 3x3x3/16 alone; A500 2x2x1/8
#      lists the square tube HSS 2x2x1/8; A500 4x2x3/16 the rectangular
#      tube HSS 4x2x3/16 alone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-tube-picker-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Python python.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Python = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-tube-picker'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'build.profile' = 'F3' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows tube picker test (skipped)'
  exit 0
}
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp); pass -Cli" }
if (-not $Python) {
  $found = Get-Command python -ErrorAction SilentlyContinue
  if ($found) { $Python = $found.Source }
}
if (-not $Python) { Ui-Fail 'no Python to run the library generator; pass -Python' }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:PYTHONDONTWRITEBYTECODE = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
$library = Join-Path $work 'mitcad-parts'
$supplier = Join-Path $work 'supplier'
New-Item -ItemType Directory -Force $supplier | Out-Null

# Run-Tool WHAT PROGRAM ARGUMENTS: a program of the setup, its output shown
# when it fails (its standard error is no PowerShell error).
function Run-Tool([string]$What, [string]$Program, [string[]]$Arguments) {
  $ErrorActionPreference = 'Continue'
  $output = & $Program @Arguments 2>&1 | ForEach-Object { "$_" }
  $status = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  if ($status -ne 0) { Ui-Fail "${What} failed ($status): $($output -join ' / ')" }
}
# The A500 tubes of the parts library (model parts: no supplier's files).
Run-Tool 'the generator' $Python @((Join-Path $PSScriptRoot 'libraries\make-profile-library.py'), '--cli', $Cli,
  '--source', $supplier, '--out', $library, '--only', 'hss-square-a500,hss-rect-a500', '--work', $work,
  '--version', '1.0.0', '--no-render', '--commit')
Run-Tool 'the library fetch' $Cli @('library', 'fetch', $library)

# Picker-Click TEXT: a click on a place of the part picker that it logged
# ("Part picker place search at x,y; in the picker x,y"), posted to the
# picker's own window at its own coordinates (tools/ui-windows-build-test.ps1).
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

# Search TEXT: the picker's search field clicked, the text searched before
# taken out and TEXT typed. $script:Result is what the picker logged for
# it, the first "Part picker group <group>: <n> parts, first <tile>" after
# its "Part picker search '<TEXT>'" line from "<n> parts" on,
# $script:Tiles the tiles it then listed ("Part picker tiles: ").
$script:Searched = ''
function Search([string]$Text) {
  Ui-Step 'the search field'             { Picker-Click 'Part picker place search' }
  Ui-Mark
  Ui-Step "search '$Text'" {
    if ($script:Searched.Length -gt 0) { Picker-Keys @(0x23) 1; Picker-Keys @(0x08) $script:Searched.Length }
    Picker-Type $Text
  }
  $script:Searched = $Text
  Ui-ExpectNew "Part picker search '$Text'" "the picker's search '$Text'"
  $script:Result = $null
  $script:Tiles = $null
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while ($clock.ElapsedMilliseconds -lt 10000) {
    $lines = @(Ui-LogLines)
    $start = -1
    for ($i = $lines.Count - 1; $i -ge 0; $i--) {
      if ($lines[$i].Contains("Part picker search '$Text'")) { $start = $i; break }
    }
    $result = $null
    $tiles = $null
    for ($i = $start + 1; $start -ge 0 -and $i -lt $lines.Count; $i++) {
      if (-not $result -and $lines[$i] -match '^Part picker group .*: (\d+ parts, first .*)$') { $result = $Matches[1] }
      if (-not $tiles -and $lines[$i].StartsWith('Part picker tiles: ')) { $tiles = $lines[$i] }
    }
    if ($result -and $tiles) { $script:Result = $result.TrimEnd("`r"); $script:Tiles = $tiles; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "no group or tiles logged after the search '$Text'"
}

Ui-StartApp 'tube-picker'
Ui-Step 'the BUILD tab'                  { Ui-ClickLogged 'Ribbon tab BUILD' }

Write-Host "--- 1. Profile's part picker"
Ui-Mark
Ui-Step 'Profile (F3)'                   { Ui-Key 'F3' }
Ui-ExpectNew 'Command Profile started' 'Profile started'
Ui-ExpectNew 'BUILD parts profile: ' "Profile lists the library's tubes"
Ui-Mark
Ui-Step 'the part picker (the Component tile)' { Ui-ClickLogged 'Panel Profile input part' }
Ui-ExpectNew 'Profile: Component button' 'the Component tile opens the picker'
Ui-ExpectNew 'Part picker opened: ' 'the part picker'
$tubes = Ui-LastMatch '^Part picker opened: (\d+) parts for build\.profile$'
if (-not $tubes -or [int]$tubes -le 100) { Ui-Fail "the picker opened with '$tubes' parts, not the A500 tubes' rows" }
Write-Host "ok   $tubes tiles, a row each"

Write-Host '--- 2. The search A500'
Search 'A500'
if (-not $script:Result.StartsWith("$tubes parts, first mitcad-parts/hss-")) {
  Ui-Fail "A500 lists '$($script:Result)', not every one of the $tubes tubes"
}
Write-Host "ok   A500 lists all $tubes tubes"

Write-Host '--- 3. HSS 3x3, 2x2 and a rectangular one'
Search 'A500 3x3x3/16'
if ($script:Result -ne '1 parts, first mitcad-parts/hss-square-a500 (3x3x3/16)') {
  Ui-Fail "A500 3x3x3/16 lists '$($script:Result)', not the square tube HSS 3x3x3/16 alone"
}
Write-Host 'ok   A500 3x3x3/16: HSS 3x3x3/16'
Search 'A500 2x2x1/8'
if (-not $script:Tiles.Contains('mitcad-parts/hss-square-a500 (2x2x1/8)')) {
  Ui-Fail "A500 2x2x1/8 lists '$($script:Tiles)', without the square tube HSS 2x2x1/8"
}
Write-Host "ok   A500 2x2x1/8: HSS 2x2x1/8 among $($script:Result)"
Search 'A500 4x2x3/16'
if ($script:Result -ne '1 parts, first mitcad-parts/hss-rect-a500 (4x2x3/16)') {
  Ui-Fail "A500 4x2x3/16 lists '$($script:Result)', not the rectangular tube HSS 4x2x3/16 alone"
}
Write-Host 'ok   A500 4x2x3/16: HSS 4x2x3/16'

Ui-Finish 'UI Windows tube picker test'
