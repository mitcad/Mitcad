# SPDX-License-Identifier: MIT
# check-all sources: app/library app/files app/commands/DerivedPartCommands.cpp app/build/BuildCatalog.cpp tools/cli
# check-all sources: tools/libraries/make-profile-library.py tools/ui-scripts/cut-length-groups.json
# check-all sources: tools/ui-scripts/cut-length-profile.json tools/ui-scripts/cut-length-rail.json
# Input-driven Windows UI test of the components cut to length from the
# library (mitcad#1950, GitHub mitcad/Mitcad#398) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted); the Linux test is
# tools/ui-cut-length-library-test.sh, with the same steps. A slot profile
# and a rail made by tools/libraries/make-profile-library.py from supplier
# files made here (tools/ui-scripts/cut-length-*.json written as STEP),
# fetched from a local git repository; skipped where git is not installed:
#   1. Insert from Library on the profile: its length L an option from
#      50 mm to 3000 mm at its design's 100 mm, its stock lengths above it;
#      L 750 mm typed and inserted with it: 261.345576 mm2 times 750 mm.
#   2. Derived Component (a double-click on its feature in the timeline):
#      L 1200 mm, OK: the profile follows.
#   3. Insert from Library on the rail: Stock length 600 mm sets L to
#      600 mm (the row 600), inserted: 15 x 12 x 600.
#   4. Insert from Library on a timber board of sizes (a closed table): its
#      Thickness and Width by its rows, L beside them; Stock length 3300 mm
#      sets L, inserted in its row 22x100 with L alone: 22 x 100 x 3300.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-cut-length-library-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Python python.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Python = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cut-length-library'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'insert.library_component' = 'F9' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows cut length library test (skipped)'
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
$scripts = Join-Path $PSScriptRoot 'ui-scripts'
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
$groups = Join-Path $work 'groups'
New-Item -ItemType Directory -Force $supplier, $groups | Out-Null

# Run-Tool WHAT PROGRAM ARGUMENTS: a program of the setup, its output shown
# when it fails (its standard error is no PowerShell error); the output.
function Run-Tool([string]$What, [string]$Program, [string[]]$Arguments) {
  $ErrorActionPreference = 'Continue'
  $output = & $Program @Arguments 2>&1 | ForEach-Object { "$_" }
  $status = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  if ($status -ne 0) { Ui-Fail "${What} failed ($status): $($output -join ' / ')" }
  return , $output
}
# The supplier's files and the group file, then the library, fetched.
foreach ($name in @('profile', 'rail')) {
  Run-Tool "mitcad-cli: the $name" $Cli @('run', (Join-Path $scripts "cut-length-$name.json"), '--save',
    (Join-Path $work "$name.mitcad")) | Out-Null
}
Run-Tool "the profile's STEP" $Cli @('export', (Join-Path $work 'profile.mitcad'), (Join-Path $supplier 'p20x20.step')) |
  Out-Null
Run-Tool "the rail's STEP" $Cli @('export', (Join-Path $work 'rail.mitcad'), (Join-Path $supplier 'rail.step')) | Out-Null
Copy-Item (Join-Path $scripts 'cut-length-groups.json') (Join-Path $groups 'a-cut-length.json')
$made = Run-Tool 'the generator' $Python @((Join-Path $PSScriptRoot 'libraries\make-profile-library.py'), '--cli', $Cli,
  '--source', $supplier, '--out', $library, '--groups', $groups, '--work', $work, '--version', '1.0.0', '--no-render',
  '--commit')
if (-not (($made -join "`n").Contains('mitcad-parts 1.0.0: 3 components'))) {
  Ui-Fail "the generator did not make all three components: $($made -join ' / ')"
}
Run-Tool 'the library fetch' $Cli @('library', 'fetch', $library) | Out-Null
$sceneScript = Join-Path $work 'scene.json'
Set-Content -Encoding ascii $sceneScript '[{"cmd": "add_parameter", "name": "Spare", "expression": "1 mm"}]'
$scene = Join-Path $work 'scene.mitcad'
Run-Tool 'mitcad-cli: the scene' $Cli @('run', $sceneScript, '--save', $scene) | Out-Null
Write-Host 'ok   the library of the profile, the rail and the board'

Ui-StartApp 'cut-length-library' @('--open', $scene)

Write-Host "--- 1. Insert from Library: the profile's length an option"
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Browse Components$'
Ui-Step 'search the profile'             { Ui-Type 'Profile 20x20 cut test' }
Ui-ExpectNew "Library search 'Profile 20x20 cut test': 1 components" 'the search finds it'
Ui-ExpectNew 'Library browser selected mitcad-parts/p20x20-cut-test: version v1.0.0 (' 'the profile selected'
Ui-ExpectNew 'Library browser options of mitcad-parts/p20x20-cut-test: L = 100' "L starts at the design's length"
Ui-ExpectNew '(row 100 mm)' 'its default row'
Ui-ExpectNew 'Library option L allowed 50 mm ' "L from the profile's shortest"
Ui-ExpectNew ' 3000 mm' 'to its longest'
Ui-ExpectNew ', Stock length ' 'the stock lengths above L'
Ui-Mark
Ui-Step 'L 750 (Alt+V, Tab, Tab, type, Tab)' {
  Ui-Key 'alt+v'; Ui-Key 'Tab'; Ui-Key 'Tab'; Ui-Key 'ctrl+a'; Ui-Type '750'; Ui-Key 'Tab'
}
Ui-ExpectNew 'Library option L chosen: 750 mm' '750 mm chosen'
Ui-ExpectNew 'Library options: L = 750' 'L 750 mm'
Ui-ExpectNew '(no row)' 'a length of no stock row'
Ui-Mark
Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Inserted component ' 'the profile inserted'
Ui-ExpectNew 'with options L = 750 mm' 'with the length typed'
Ui-ExpectVolume 'New body [^(]* \([^)]*\): volume ([0-9.]+) mm3' 196009.182298 'the profile 750 mm long'

Write-Host '--- 2. Derived Component: its length changed'
Ui-Mark
Ui-Step 'double-click its feature'       { Ui-DoubleClickLogged 'Timeline DerivedComponent1' }
Ui-ExpectNew ' with Derived Component' 'Derived Component opens on its feature'
Ui-ExpectNew ', options, rows 100 mm|3000 mm|1000 mm, ' "the profile's rows"
Ui-ExpectNew ', parameters L, ' 'its length L among its values'
Ui-Mark
Ui-Step 'L = 1200 mm'                    { Ui-TypeIn 'Panel Derived Component input param0' '1200 mm' }
Ui-Step 'OK'                             { Ui-ClickLogged 'Panel Derived Component OK' }
Ui-ExpectNew ' edited: L = 1200 mm' 'one command with the new length'
Ui-ExpectVolume 'Body [^(]* \([^)]*\): volume [0-9.]+ -> ([0-9.]+) mm3' 313614.691677 'the profile follows, 1200 mm'

Write-Host '--- 3. The rail: a stock length sets its length'
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Browse Components$'
Ui-Step 'search the rail'                { Ui-Type 'Rail cut test' }
Ui-ExpectNew "Library search 'Rail cut test': 1 components" 'the search finds the rail'
Ui-ExpectNew 'Library browser selected mitcad-parts/rail-cut-test: version v1.0.0 (' 'the rail selected'
Ui-ExpectNew 'Library browser options of mitcad-parts/rail-cut-test: L = 300' 'the rail at its first stock row'
Ui-Mark
Ui-Step 'Stock length 600 (Alt+V, Tab, 6)' { Ui-Key 'alt+v'; Ui-Key 'Tab'; Ui-Type '6' }
Ui-ExpectNew 'Library stock length 600: 600 mm' 'the stock length 600 mm chosen'
Ui-ExpectNew 'Library options: L = 600' 'L set to 600 mm'
Ui-ExpectNew '(row 600)' 'the row 600'
Ui-ExpectNew 'Library browser row 600: designation ' "the row's designation"
Ui-Mark
Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Inserted component ' 'the rail inserted'
Ui-ExpectNew 'with options L = 600 mm' 'with its stock length'
Ui-ExpectVolume 'New body [^(]* \([^)]*\): volume ([0-9.]+) mm3' 108000 'the rail 15 x 12 x 600'

Write-Host '--- 4. A board of sizes: L beside its closed rows'
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Browse Components$'
Ui-Step 'search the board'               { Ui-Type 'Board cut test' }
Ui-ExpectNew "Library search 'Board cut test': 1 components" 'the search finds the board'
Ui-ExpectNew 'Library browser selected mitcad-parts/board-cut-test: version v1.0.0 (' 'the board selected'
Ui-ExpectNew 'Library browser options of mitcad-parts/board-cut-test: Thickness = 22, Width = 100, L = 3000' "its sizes by its default row, L at the design's length"
Ui-ExpectNew '(row 22x100)' 'the row 22x100'
Ui-ExpectNew 'Library option Width allowed 100, 150 (Only these rows)' 'Width by the rows of 22'
Ui-ExpectNew 'Library option L allowed 100 mm ' "L from the board's shortest"
Ui-Mark
Ui-Step 'Stock length 3300 (Alt+V, Tab, 33)' { Ui-Key 'alt+v'; Ui-Key 'Tab'; Ui-Type '33' }
Ui-ExpectNew 'Library stock length 3300: 3300 mm' 'the stock length 3300 mm chosen'
Ui-ExpectNew 'Library options: Thickness = 22, Width = 100, L = 3300' 'L set to 3300 mm, the size kept'
Ui-ExpectNew '(row 22x100)' 'still the row 22x100'
Ui-Mark
Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Inserted component ' 'the board inserted'
Ui-ExpectNew 'with options L = 3300 mm (row 22x100)' 'in its row with its length alone'
Ui-ExpectVolume 'New body [^(]* \([^)]*\): volume ([0-9.]+) mm3' 7260000 'the board 22 x 100 x 3300'

Ui-Finish 'UI Windows cut length library test'
