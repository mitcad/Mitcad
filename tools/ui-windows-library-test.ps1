# SPDX-License-Identifier: MIT
# check-all sources: app/files app/library tools/cli
# Input-driven Windows UI test of the library browser's Update button
# (mitcad#317) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted); the Linux test is tools/ui-library-test.sh (parts 2b and 6).
# Local git repositories only, no network; skipped where git is not
# installed (fetching needs it):
#   1. Libraries...: the default sources removed, a library of one block
#      (mitcad-cli library init/add, one git commit) added, not fetched.
#   2. Insert from Library: empty, its text says to press Update; Update
#      fetches the source and lists the block.
#   3. A source that is no library added: Update fetches the other, says so
#      in one warning, searches again and keeps the selected block.
#   4. The part picker's grid (mitcad#418): the group Blocks, a search
#      inside it, a star, Favourites, the block inserted from there and In
#      this design with it.
#   5. The groups as a tree (mitcad#527): two more libraries, one's Wood (a
#      parent by id) and the other's Wood/Sheets one group listing both
#      libraries' parts, a screw in two groups (Slot profile/Screws and
#      General fasteners/Screws) and once in Favourites, Wood folded and
#      still folded when the browser opens again, then unfolded.
#   6. The Standard filter (mitcad#949): a socket head cap screw whose rows
#      name their standards; Standard = DIN 912 narrows the lengths of M6 to
#      8 and 10, the row's two supplier articles in their table (Supplier
#      Articles, mitcad#979), and the part inserted is named DIN 912 M6x8.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-library-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-library'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.libraries' = 'F8'; 'insert.library_component' = 'F9' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows library test (skipped)'
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
# The fetched libraries of the test's own.
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
# A supplier's site logged, not opened (Open Link, mitcad#979).
$env:MITCAD_TEST_LOG_URLS = '1'

# A block (20 x 20 x 10) in a library of its own, recorded with git.
$script = Join-Path $work 'block.json'
Set-Content -Encoding ascii $script @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
'@
$block = Join-Path $work 'block.mitcad'
$output = & $cli run $script --save $block
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $block)) { Ui-Fail "mitcad-cli run: $($output -join ' / ')" }
$library = Join-Path $work 'blocks'
$output = & $cli library init $library --id test-blocks --name 'Test blocks' --license CC0-1.0
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library init: $($output -join ' / ')" }
$output = & $cli library add $library $block --id block --name 'Test block' --category blocks --license CC0-1.0
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library add: $($output -join ' / ')" }
foreach ($arguments in @(@('add', '-A'), @('commit', '-q', '-m', 'Blocks'))) {
  $output = & $git.Source -C $library -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
$broken = Join-Path $work 'no-such-library'

# Type-Ahead text: the picker's group by its first letters. Qt joins the
# keys typed within its keyboard input interval (400 ms) into one search,
# also across steps ('W', then 'Slot' would look for 'WSlot'): it waits
# that long first, as the Linux test's ui_type_ahead.
function Type-Ahead([string]$Text) {
  Start-Sleep -Milliseconds 500
  Ui-Type $Text
}

# Add-Source path: Add URL in Libraries... (open), then back to it.
function Add-Source([string]$Path) {
  Ui-Step 'Add URL (Alt+A)'              { Ui-Key 'alt+a' }
  Ui-FocusDialog '^Add Library$'
  Ui-Step 'the URL'                      { Ui-Type $Path; Ui-Key 'Return' }
  Ui-FocusDialog '^Libraries$'
  Ui-ExpectNew "Library source added: $Path" 'the source added'
}

Ui-StartApp 'library' @('--open', $block)

Write-Host '--- 1. Libraries: only the block library, not fetched'
Ui-Mark
Ui-Step 'Libraries (F8)'                 { Ui-Key 'F8' }
Ui-FocusDialog '^Libraries$'
Ui-ExpectNew 'Library source https://github.com/mitcad/parts.git (on): not fetched' 'the default sources listed'
Ui-Step 'remove a default (Alt+R)'       { Ui-Key 'alt+r' }
Ui-Step 'and the other (Alt+R)'          { Ui-Key 'alt+r' }
Ui-ExpectNew 'Library source removed: https://github.com/mitcad/community-index.git' 'the defaults removed'
Add-Source $library
Ui-Step 'close (Alt+C)'                  { Ui-Key 'alt+c' }
Ui-FocusMain

Write-Host '--- 2. Insert from Library: empty until Update (mitcad#317)'
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-ExpectNew "Library search '': 0 components, 0 libraries" 'nothing listed before a fetch'
Ui-ExpectNew 'Library browser empty: nothing fetched yet, press Update' 'the empty list says to press Update'
Ui-Mark
Ui-Step 'Update (Alt+U)'                 { Ui-Key 'alt+u' }
Ui-ExpectNew 'Library browser update: 1 enabled source(s)' 'Update fetches the enabled source'
Ui-ExpectNew "Library fetched: library Test blocks (test-blocks) from $library" 'the library fetched' 30
Ui-ExpectNew 'Library fetch of 1 source(s): 0 failed' 'no failures' 30
Ui-ExpectNew "Library search '': 1 components" 'the search ran again'
Ui-ExpectNew 'Library browser selected test-blocks/block: version ' 'the block listed and selected'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 3. A broken source: one warning, the selection kept'
Ui-Mark
Ui-Step 'Libraries (F8)'                 { Ui-Key 'F8' }
Ui-FocusDialog '^Libraries$'
Add-Source $broken
Ui-Step 'close (Alt+C)'                  { Ui-Key 'alt+c' }
Ui-FocusMain
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-ExpectNew 'Library browser selected test-blocks/block: version ' 'the block selected'
Ui-Mark
Ui-Step 'Update (Alt+U)'                 { Ui-Key 'alt+u' }
Ui-ExpectNew 'Library browser update: 2 enabled source(s)' 'Update fetches both sources'
Ui-ExpectNew "Library fetch failed: ${broken}: " 'the broken one fails' 30
Ui-ExpectNew 'Library fetch of 2 source(s): 1 failed' 'one failure' 30
Ui-ExpectNew 'Library browser kept the selection component|' 'the search ran again, the block kept'
Ui-FocusDialog '^Fetch Libraries$'
Ui-Step 'close the warning (Return)'     { Ui-Key 'Return' }
Ui-FocusDialog '^Insert from Library$'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 4. The part picker: a group, a search, a star, Favourites, In this design (mitcad#418)'
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-ExpectNew 'Part picker opened: 1 parts for Insert from Library' 'the results as the picker grid'
Ui-ExpectNew 'Part picker in this design: 0 parts' 'no library part in the design yet'
Ui-Mark
Ui-Step 'the group Blocks (Alt+O, B)'    { Ui-Key 'alt+o'; Type-Ahead 'B' }
Ui-ExpectNew 'Part picker group Blocks: 1 parts, first test-blocks/block' 'the group'
Ui-Mark
Ui-Step 'search for nothing (Alt+S)'     { Ui-Key 'alt+s'; Ui-Type 'nosuchpart' }
Ui-ExpectNew "Library search 'nosuchpart': 0 components" 'the search'
Ui-ExpectNew 'Part picker group Blocks: 0 parts, first -' 'nothing in the group'
Ui-Mark
Ui-Step 'clear the search'               { Ui-Key 'ctrl+a'; Ui-Key 'BackSpace' }
Ui-ExpectNew 'Part picker group Blocks: 1 parts, first test-blocks/block' 'the block again'
Ui-ExpectNew 'Library browser selected test-blocks/block: version ' 'the block chosen in the grid'
Ui-Mark
Ui-Step 'star it (Ctrl+D)'               { Ui-Key 'ctrl+d' }
Ui-ExpectNew 'Part picker star block on' 'the block starred'
Ui-ExpectNew 'Part picker tiles kept: ' 'a star changes only the star: the tiles stay (mitcad#517)'
Ui-ExpectNew '; current test-blocks/block' 'the selection stays on the starred block'
Ui-Mark
Ui-Step 'Favourites (Alt+O, F)'          { Ui-Key 'alt+o'; Type-Ahead 'F' }
Ui-ExpectNew 'Part picker group Favourites: 1 parts, first test-blocks/block' 'Favourites has the block'
Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Part picker chose test-blocks/block' 'chosen in the picker'
Ui-ExpectNew 'Inserted component Test block (linked) from test-blocks ' 'the block inserted'
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-ExpectNew 'Part picker in this design: 1 parts: test-blocks/block x1' 'the block in this design'
Ui-ExpectNew 'Part picker group All parts: 1 parts, first test-blocks/block' 'All parts again'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 5. The groups as a tree (mitcad#527)'
# Two libraries more: Wood (a parent by id) with its Beams, and a screw
# made for the slot in two groups, Slot profile/Screws and General
# fasteners/Screws; the other's Wood/Sheets in the same Wood.
$wood = Join-Path $work 'wood'
$sheets = Join-Path $work 'sheets'
foreach ($folder in @((Join-Path $wood 'beams'), (Join-Path $wood 'slot-screws'), (Join-Path $sheets 'sheets'))) {
  New-Item -ItemType Directory -Force $folder | Out-Null
}
Copy-Item $block (Join-Path $wood 'beams\beam.mitcad')
Copy-Item $block (Join-Path $wood 'slot-screws\slot-screw.mitcad')
Copy-Item $block (Join-Path $sheets 'sheets\plywood.mitcad')
Set-Content -Encoding ascii (Join-Path $wood 'mitcad-library.json') @'
{"format": "mitcad-library", "version": 1, "id": "test-wood", "name": "Test wood", "license": "MIT",
 "categories": [{"id": "wood", "name": "Wood"}, {"id": "beams", "name": "Beams", "parent": "wood"},
                {"id": "slot-screws", "name": "Slot profile/Screws"},
                {"id": "general-screws", "name": "General fasteners/Screws"}],
 "components": [
   {"id": "beam", "path": "beams/beam.mitcad", "name": "Beam test", "category": "beams"},
   {"id": "slot-screw", "path": "slot-screws/slot-screw.mitcad", "name": "Slot screw test",
    "category": ["slot-screws", "general-screws"]}]}
'@
Set-Content -Encoding ascii (Join-Path $sheets 'mitcad-library.json') @'
{"format": "mitcad-library", "version": 1, "id": "test-sheets", "name": "Test sheets", "license": "MIT",
 "categories": [{"id": "sheets", "name": "Wood/Sheets"}],
 "components": [{"id": "plywood", "path": "sheets/plywood.mitcad", "name": "Plywood test", "category": "sheets"}]}
'@
foreach ($folder in @($wood, $sheets)) {
  Set-Content -Encoding ascii (Join-Path $folder 'LICENSE') 'MIT'
  $output = & $cli library check $folder
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library check ${folder}: $($output -join ' / ')" }
  foreach ($arguments in @(@('init', '-q'), @('add', '-A'), @('commit', '-q', '-m', 'Parts'))) {
    $output = & $git.Source -C $folder -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
    if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
  }
}
Ui-Mark
Ui-Step 'Libraries (F8)'                 { Ui-Key 'F8' }
Ui-FocusDialog '^Libraries$'
Add-Source $wood
Add-Source $sheets
Ui-Step 'close (Alt+C)'                  { Ui-Key 'alt+c' }
Ui-FocusMain
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-Mark
Ui-Step 'Update (Alt+U)'                 { Ui-Key 'alt+u' }
Ui-ExpectNew "Library fetched: library Test wood (test-wood) from $wood" 'the wood library fetched' 30
Ui-ExpectNew "Library fetched: library Test sheets (test-sheets) from $sheets" 'the sheets fetched' 30
Ui-ExpectNew 'Library fetch of 4 source(s): 1 failed' 'only the broken source fails' 30
Ui-FocusDialog '^Fetch Libraries$'
Ui-Step 'close the warning (Return)'     { Ui-Key 'Return' }
Ui-FocusDialog '^Insert from Library$'
Ui-ExpectNew ', Wood/Sheets' "the groups as a tree, the other library's Sheets in the same Wood"
$groupsTree = Ui-LastMatch '^Part picker groups: (.*)$'
foreach ($group in @('Wood', 'Wood/Beams', 'Wood/Sheets', 'Slot profile', 'Slot profile/Screws',
                     'General fasteners', 'General fasteners/Screws', 'Blocks')) {
  if (", $groupsTree, " -notlike "*, $group, *") { Ui-Fail "no group $group in '$groupsTree'" }
}
Write-Host "ok   the groups as a tree: $groupsTree"
Ui-Mark
Ui-Step 'the group Wood (Alt+O, W)'      { Ui-Key 'alt+o'; Type-Ahead 'W' }
Ui-ExpectNew 'Part picker group Wood: 2 parts' "Wood lists its subgroups' parts, of both libraries"
Ui-Mark
Ui-Step "Slot profile's Screws (Alt+O, Slot, Down)" { Ui-Key 'alt+o'; Type-Ahead 'Slot'; Ui-Key 'Down' }
Ui-ExpectNew 'Part picker group Slot profile/Screws: 1 parts, first test-wood/slot-screw' 'the slot screw in Slot profile'
Ui-Mark
Ui-Step "General fasteners' Screws (Alt+O, General, Down)" { Ui-Key 'alt+o'; Type-Ahead 'General'; Ui-Key 'Down' }
Ui-ExpectNew 'Part picker group General fasteners/Screws: 1 parts, first test-wood/slot-screw' 'the same screw in General fasteners'
# The slot screw stays the current tile (chosen in Slot profile/Screws: the
# browser logs no new selection); the star names it.
Ui-Mark
Ui-Step 'star it (Ctrl+D)'               { Ui-Key 'ctrl+d' }
Ui-ExpectNew 'Part picker star slot-screw on' 'the slot screw starred'
Ui-Mark
Ui-Step 'Favourites (Alt+O, F)'          { Ui-Key 'alt+o'; Type-Ahead 'F' }
Ui-ExpectNew 'Part picker group Favourites: 2 parts' 'the block and the slot screw'
$tiles = Ui-LastMatch '^Part picker tiles: (.*)$'
if (([regex]::Matches("$tiles", 'test-wood/slot-screw')).Count -ne 1) {
  Ui-Fail "the slot screw is not in Favourites once: '$tiles'"
}
Write-Host 'ok   a part in two groups is one tile in Favourites'
Ui-Mark
Ui-Step 'fold Wood (Alt+O, W, Left)'     { Ui-Key 'alt+o'; Type-Ahead 'W'; Ui-Key 'Left' }
Ui-ExpectNew 'Part picker group Wood folded' 'Wood folded'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-ExpectNew 'Wood (folded)' 'the fold remembered'
$groupsTree = Ui-LastMatch '^Part picker groups: (.*)$'
if ("$groupsTree" -like '*Wood/Beams*') { Ui-Fail "a folded group's subgroups are shown: '$groupsTree'" }
Ui-Mark
Ui-Step 'unfold it (Alt+O, W, Right)'    { Ui-Key 'alt+o'; Type-Ahead 'W'; Ui-Key 'Right' }
Ui-ExpectNew 'Part picker group Wood unfolded' 'Wood unfolded'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 6. The Standard filter (mitcad#949)'
# A socket head cap screw of four rows, tagged with their standards as the
# parts library's (Linux: tools/ui-library-test.sh, part 9): M5x10 and M6x10
# of ISO 4762 and DIN 912, M6x8 of DIN 912 only, M6x6 of no standard. With
# Standard = DIN 912 the lengths of M6 are 8 and 10, and the part inserted
# is named by that standard.
$screws = Join-Path $work 'screws'
New-Item -ItemType Directory -Force (Join-Path $screws 'screws') | Out-Null
$screwScript = Join-Path $work 'screw.json'
Set-Content -Encoding ascii $screwScript @'
[
  {"cmd": "add_parameter", "name": "d", "expression": "6 mm"},
  {"cmd": "add_parameter", "name": "L", "expression": "10 mm"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": "d", "height": "d"},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": "L"}, "operation": "new_body"}},
  {"cmd": "set_configurations", "configurations": {
    "selectors": ["Size", "Length"], "parameters": ["d", "L"], "default": "M5x10",
    "rows": [
      {"name": "M5x10", "select": {"Size": "M5", "Length": "10"}, "values": {"d": "5 mm", "L": "10 mm"}},
      {"name": "M6x6", "select": {"Size": "M6", "Length": "6"}, "values": {"d": "6 mm", "L": "6 mm"}},
      {"name": "M6x8", "select": {"Size": "M6", "Length": "8"}, "values": {"d": "6 mm", "L": "8 mm"}},
      {"name": "M6x10", "select": {"Size": "M6", "Length": "10"}, "values": {"d": "6 mm", "L": "10 mm"}}]}}
]
'@
$screwDesign = Join-Path $screws 'screws\iso4762.mitcad'
$output = & $cli run $screwScript --save $screwDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $screwDesign)) { Ui-Fail "mitcad-cli run: $($output -join ' / ')" }
Set-Content -Encoding ascii (Join-Path $screws 'mitcad-library.json') @'
{"format": "mitcad-library", "version": 1, "id": "test-screws", "name": "Test screws", "license": "MIT",
 "suppliers": [{"id": "acme", "name": "Acme", "url": "https://acme.example.org"}],
 "categories": [{"id": "screws", "name": "General fasteners/Screws"}],
 "components": [
   {"id": "iso4762", "path": "screws/iso4762.mitcad", "name": "Hexagon socket head cap screw",
    "category": "screws", "standard": "ISO 4762 / DIN 912", "designation": "{standard} {Size}x{Length}",
    "rows": {
      "M5x10": {"standards": ["ISO 4762", "DIN 912"]},
      "M6x6": {"standards": ["non-standard"]},
      "M6x8": {"standards": ["DIN 912"],
               "supplier_parts": [{"supplier": "acme", "part_number": "A 6 8", "material": "8.8",
                                   "finish": "zinc-plated", "checked": "2026-10-30"},
                                  {"supplier": "acme", "part_number": "A 6 8 S", "material": "A2",
                                   "finish": "plain", "checked": "2026-10-30"}]},
      "M6x10": {"standards": ["ISO 4762", "DIN 912"]}}}]}
'@
Set-Content -Encoding ascii (Join-Path $screws 'LICENSE') 'MIT'
$output = & $cli library check $screws
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library check ${screws}: $($output -join ' / ')" }
foreach ($arguments in @(@('init', '-q'), @('add', '-A'), @('commit', '-q', '-m', 'Screws'))) {
  $output = & $git.Source -C $screws -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
}
Ui-Mark
Ui-Step 'Libraries (F8)'                 { Ui-Key 'F8' }
Ui-FocusDialog '^Libraries$'
Add-Source $screws
Ui-Step 'close (Alt+C)'                  { Ui-Key 'alt+c' }
Ui-FocusMain
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Insert from Library$'
Ui-Mark
Ui-Step 'Update (Alt+U)'                 { Ui-Key 'alt+u' }
Ui-ExpectNew "Library fetched: library Test screws (test-screws) from $screws" 'the screws fetched' 30
Ui-ExpectNew 'source(s): 1 failed' 'only the broken source fails' 30
Ui-FocusDialog '^Fetch Libraries$'
Ui-Step 'close the warning (Return)'     { Ui-Key 'Return' }
Ui-FocusDialog '^Insert from Library$'
Ui-Step 'All parts (Alt+O, A)'           { Ui-Key 'alt+o'; Type-Ahead 'A' }
Ui-Mark
Ui-Step 'search the screw (Alt+S)'       { Ui-Key 'alt+s'; Ui-Type 'socket head' }
Ui-ExpectNew 'Library browser selected test-screws/iso4762: version ' 'the socket head cap screw'
Ui-Mark
Ui-Step 'Standard DIN 912 (Alt+D)'       { Ui-Key 'alt+d'; Type-Ahead 'DIN' }
Ui-ExpectNew 'Library filter: Standard DIN 912, Supplier All: 3 of 4 rows' 'the Standard filter narrows the rows'
Ui-Mark
# Size M6: the field before Length, which is before Linked.
Ui-Step 'size M6 (Alt+L, Shift+Tab twice)' { Ui-Key 'alt+l', 'shift+Tab', 'shift+Tab'; Ui-Type 'M6' }
Ui-ExpectNew 'Library Length choices (Standard DIN 912, Supplier All): 8, 10' 'the lengths of M6 narrowed'
Ui-Mark
# The first length (8): Home, as a typed 8 looks on from the current length.
Ui-Step 'length 8 (Tab, Home)'           { Ui-Key 'Tab', 'Home' }
Ui-ExpectNew 'Library size M6x8' 'M6x8 chosen'
Ui-ExpectNew 'Library browser row M6x8: designation DIN 912 M6x8; supplier part numbers Acme A 6 8 (8.8 zinc-plated); Acme A 6 8 S (A2 plain)' `
  'the designation follows the standard; the supplier part numbers'
# The row's articles in their table, not as one line (mitcad#979).
Ui-Mark
Ui-Step 'Supplier Articles (Alt+A)'      { Ui-Key 'alt+a' }
Ui-FocusDialog '^Supplier Articles - DIN 912 M6x8$'
Ui-ExpectNew 'Supplier parts of M6x8: 2 articles, columns Supplier, Part number, Material, Finish, Checked' `
  'the table of the two articles and the columns they fill'
Ui-Step 'Copy as TSV (Alt+T)'            { Ui-Key 'alt+t' }
Ui-ExpectNew 'Supplier articles copied: 2 rows' 'both articles copied'
Ui-Step 'Open Link (Alt+L)'              { Ui-Key 'alt+l' }
Ui-ExpectNew 'Supplier link opened: https://acme.example.org' "the supplier's site"
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusDialog '^Insert from Library$'
Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Inserted component DIN 912 M6x8 (linked) from test-screws' 'the part named by DIN 912'

Ui-Finish 'UI Windows library test'
