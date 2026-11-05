# SPDX-License-Identifier: MIT
# check-all sources: app/files app/library app/MainWindow.cpp tools/cli
# Input-driven Windows UI test of Browse Components (mitcad#1283, GitHub
# #136) in the build VM (tools/ui-windows-lib.ps1 says how the input is
# posted), as tools/ui-browse-components-test.sh does on Linux. Local
# folder sources only, no network; skipped where git is not installed
# (fetching a folder needs it): a library of a block (a source) and a
# community index that names a library of a plate (not a source).
#   1. Tools > Libraries (Alt+T, L) lists Browse Components... once, as
#      its first entry; it opens the one window, titled Browse Components,
#      on Installed.
#   2. The browser's Library Sources... opens the Library Sources window:
#      the default sources removed, the block's library and the index
#      added; back in the browser Update fetches both.
#   3. Installed lists the block; Community keeps it selected; the search
#      "plate" finds the index's library and its component Boss plate,
#      whose library is not fetched: the primary button is Get Library. Get
#      Library fetches it; Boss plate stays selected, the search kept, the
#      button Insert, and Insert places it.
#   4. Installed -> Community -> Favorites (no favorites yet) -> Installed,
#      the block starred, then Favorites lists it, the selection kept.
#      Insert from Library (F9) and Community Library (F7) open the same
#      window on Installed and on Community.
#   5. The Floating layout (--chrome=floating): Tools > Libraries lists
#      Browse Components... once and opens the browser.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browse-components-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browse-components'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'insert.library_component' = 'F9'; 'tools.community_library' = 'F7' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: UI Windows Browse Components test (skipped)'
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
# The labels' ellipsis, as the log has it (this file is ASCII).
$e = [string][char]0x2026

# Ui-LoggedAfter "text": the last line since the mark with the text.
function Ui-LoggedAfter([string]$Text) {
  return Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) } | Select-Object -Last 1
}

# Git-Record folder: the folder's files recorded as one commit.
function Git-Record([string]$Folder, [string[]]$First = @()) {
  $steps = @()
  if ($First.Count -gt 0) { $steps += , $First }
  $steps += , @('add', '-A')
  $steps += , @('commit', '-q', '-m', 'Parts')
  foreach ($arguments in $steps) {
    $output = & $git.Source -C $Folder -c user.name=T -c user.email=t@example.invalid @arguments 2>&1
    if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($arguments -join ' '): $($output -join ' / ')" }
  }
}

# A block (20 x 20 x 10), the design the libraries' components are.
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

# Make-Library folder id name component title category: a library of the
# block as one component, recorded with git.
function Make-Library([string]$Folder, [string]$Id, [string]$Title, [string]$Component, [string]$Label,
                      [string]$Category) {
  $made = & $cli library init $Folder --id $Id --name $Title --license CC0-1.0
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library init ${Id}: $($made -join ' / ')" }
  $made = & $cli library add $Folder $block --id $Component --name $Label --category $Category --license CC0-1.0
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli library add ${Component}: $($made -join ' / ')" }
  Git-Record $Folder
}
$blocks = Join-Path $work 'blocks'
$plates = Join-Path $work 'plates'
Make-Library $blocks 'test-blocks' 'Test blocks' 'block' 'Test block' 'blocks'
Make-Library $plates 'test-plates' 'Test plates' 'boss-plate' 'Boss plate' 'plates'
# The community index: the plates' library with its component, not fetched.
$index = Join-Path $work 'index'
New-Item -ItemType Directory -Force (Join-Path $index 'libraries') | Out-Null
Set-Content -Encoding ascii (Join-Path $index 'mitcad-index.json') '{"format": "mitcad-index", "version": 1, "name": "Test index"}'
$entry = [ordered]@{
  format = 'mitcad-index-entry'; version = 1; id = 'test-plates'; name = 'Test plates'; url = $plates
  license = 'CC0-1.0'; tags = @('plate')
  components = @([ordered]@{ id = 'boss-plate'; name = 'Boss plate'; tags = @('plate') })
}
Set-Content -Encoding ascii (Join-Path $index 'libraries\test-plates.json') ($entry | ConvertTo-Json -Depth 5)
Git-Record $index @('init', '-q')

# Libraries-Menu: Tools > Libraries opened (Alt+T, L); Browse
# Components... once, first; Insert from Library... and Community
# Library... no longer in it.
function Libraries-Menu {
  Ui-Mark
  Ui-Step 'the Tools menu (Alt+T)'       { Ui-Key 'alt+t' }
  Ui-ExpectNew 'Tools menu: ' 'the Tools menu opened'
  Ui-Step 'Libraries (L)'                { Ui-Key 'l' }
  Ui-ExpectNew 'Libraries menu: ' 'the Libraries menu opened'
  $line = Ui-LoggedAfter 'Libraries menu: '
  $menu = $line.Substring($line.IndexOf('Libraries menu: ') + 'Libraries menu: '.Length)
  $count = ([regex]::Matches($menu, [regex]::Escape("Browse Components$e"))).Count
  if ($count -ne 1) { Ui-Fail "Browse Components... is $count times in the Libraries menu: '$menu'" }
  if (-not $menu.StartsWith("Browse Components$e | - | ")) {
    Ui-Fail "Browse Components... is not the menu's first entry: '$menu'"
  }
  if ($menu.Contains('Insert from Library') -or $menu.Contains('Community Library')) {
    Ui-Fail "the Libraries menu still has Insert from Library or Community Library: '$menu'"
  }
  Write-Host "ok   Browse Components... once, first"
}

# Add-Source path: Add URL in Library Sources... (open), then back to it.
function Add-Source([string]$Path) {
  Ui-Step 'Add URL (Alt+A)'              { Ui-Key 'alt+a' }
  Ui-FocusDialog '^Add Library$'
  Ui-Step 'the folder'                   { Ui-Type $Path; Ui-Key 'Return' }
  Ui-FocusDialog '^Library Sources$'
  Ui-ExpectNew "Library source added: $Path" 'the source added'
}

Ui-StartApp 'browse-components' @('--open', $block)

Write-Host '--- 1. Tools > Libraries: Browse Components... once'
Ui-FocusMain
Libraries-Menu
Ui-Mark
Ui-Step 'Browse Components... (B)'       { Ui-Key 'b' }
Ui-FocusDialog '^Browse Components$'
Ui-ExpectNew 'Library browser view: Installed' 'the browser opened on Installed'
Ui-ExpectNew 'Part picker opened: 0 parts for Browse Components' 'one window, titled Browse Components'

Write-Host '--- 2. Library Sources... from the browser'
Ui-Mark
Ui-Step 'Library Sources... (Alt+B)'     { Ui-Key 'alt+b' }
Ui-FocusDialog '^Library Sources$'
Ui-ExpectNew 'Library browser: Library Sources opened' "the browser's button opened it"
Ui-ExpectNew 'Library source https://github.com/mitcad/parts.git (on): not fetched' 'the default sources listed'
Ui-Step 'remove a default (Alt+R)'       { Ui-Key 'alt+r' }
Ui-Step 'and the other (Alt+R)'          { Ui-Key 'alt+r' }
Ui-ExpectNew 'Library source removed: https://github.com/mitcad/community-index.git' 'the defaults removed'
Add-Source $blocks
Add-Source $index
Ui-Step 'close (Alt+C)'                  { Ui-Key 'alt+c' }
Ui-FocusDialog '^Browse Components$'
Ui-Mark
Ui-Step 'Update (Alt+U)'                 { Ui-Key 'alt+u' }
Ui-ExpectNew "Library fetched: library Test blocks (test-blocks) from $blocks" "the block's library fetched" 30
Ui-ExpectNew "Library fetched: index Test index () from $index" 'the index fetched' 30
Ui-ExpectNew 'Library fetch of 2 source(s): 0 failed' 'no failures' 30

Write-Host '--- 3. Installed, Community, Get Library in place'
Ui-ExpectNew "Library search '': 1 components, 1 libraries" "Installed: the block and the plates' library to get"
Ui-ExpectNew 'Library browser selected test-blocks/block: version ' 'the block selected'
Ui-ExpectNew 'Library browser button: Insert' 'Insert for a component whose library is here'
Ui-Mark
Ui-Step 'Community (Alt+M)'              { Ui-Key 'alt+m' }
Ui-ExpectNew 'Library browser view: Community' 'the Community view'
Ui-ExpectNew 'Library browser: 1 components of libraries to get' "the index's component of a library to get"
Ui-ExpectNew 'Library browser kept the selection component|' 'the block kept selected'
Ui-Mark
Ui-Step 'search plate (Alt+S)'           { Ui-Key 'alt+s'; Ui-Type 'plate' }
Ui-ExpectNew "Library search 'plate': 0 components, 1 libraries" "the search finds the index's library"
Ui-ExpectNew "Library browser selected library test-plates ($plates), licence CC0-1.0, not fetched" "the plates' library first"
Ui-ExpectNew 'Library browser button: Get Library' 'Get Library for a library that is not here'
Ui-Mark
Ui-Step 'its component (Down)'           { Ui-Key 'Down' }
Ui-ExpectNew "Library browser selected test-plates/boss-plate of a library to get ($plates), licence CC0-1.0: Get Library" `
  'a component of a library not fetched: the primary button is Get Library'
Ui-Mark
Ui-Step 'Get Library (Alt+G)'            { Ui-Key 'alt+g' }
Ui-FocusDialog '^Get Library$'
Ui-Step 'yes (Alt+Y)'                    { Ui-Key 'alt+y' }
Ui-FocusDialog '^Browse Components$'
Ui-ExpectNew "Library fetched: library Test plates (test-plates) from $plates" 'the library fetched' 30
Ui-ExpectNew "Library search 'plate': 1 components, 1 libraries" 'the search kept, the fetched component listed'
Ui-ExpectNew 'Library browser fetched test-plates: selected test-plates/boss-plate' 'the same component kept selected'
Ui-ExpectNew 'Library browser selected test-plates/boss-plate: version ' 'Boss plate at its version'
Ui-ExpectNew 'Library browser button: Insert' 'now Insert'
Ui-Mark
Ui-Step 'Insert (Alt+I)'                 { Ui-Key 'alt+i' }
Ui-FocusMain
Ui-ExpectNew 'Part picker chose test-plates/boss-plate' 'chosen in the browser'
Ui-ExpectNew 'Inserted component Boss plate (linked) from test-plates ' 'Boss plate inserted'

Write-Host '--- 4. The views: Installed -> Community -> Favorites, and the old commands'
Libraries-Menu
Ui-Mark
Ui-Step 'Browse Components... (B)'       { Ui-Key 'b' }
Ui-FocusDialog '^Browse Components$'
Ui-ExpectNew 'Library browser view: Installed' 'Installed first'
Ui-Mark
Ui-Step 'Community (Alt+M)'              { Ui-Key 'alt+m' }
Ui-ExpectNew 'Library browser view: Community' 'Community'
Ui-Mark
Ui-Step 'Favorites (Alt+F)'              { Ui-Key 'alt+f' }
Ui-ExpectNew 'Library browser view: Favorites' 'Favorites'
Ui-ExpectNew 'Library browser favorites: 0 of 2 components starred' 'nothing starred yet'
Ui-ExpectNew 'Library browser empty: no favorites' 'its empty list says how to star'
Ui-Mark
Ui-Step 'Installed (Alt+N)'              { Ui-Key 'alt+n' }
Ui-ExpectNew 'Library browser view: Installed' 'back to Installed'
Ui-Mark
Ui-Step 'search block (Alt+S)'           { Ui-Key 'alt+s'; Ui-Type 'block' }
Ui-ExpectNew 'Library browser selected test-blocks/block: version ' 'the block selected'
Ui-Mark
Ui-Step 'star it (Ctrl+D)'               { Ui-Key 'ctrl+d' }
Ui-ExpectNew 'Part picker star block on' 'the block starred'
Ui-Mark
Ui-Step 'Favorites (Alt+F)'              { Ui-Key 'alt+f' }
Ui-ExpectNew 'Library browser view: Favorites' 'Favorites again'
Ui-ExpectNew 'Library browser favorites: 1 of 1 components starred' 'the starred block under the search'
Ui-ExpectNew 'Library browser kept the selection component|' 'the block kept selected'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Mark
Ui-Step 'Insert from Library (F9)'       { Ui-Key 'F9' }
Ui-FocusDialog '^Browse Components$'
Ui-ExpectNew 'Library browser view: Installed' 'Insert from Library opens it on Installed'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Mark
Ui-Step 'Community Library (F7)'         { Ui-Key 'F7' }
Ui-FocusDialog '^Browse Components$'
Ui-ExpectNew 'Library browser view: Community' 'Community Library opens it on Community'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 5. The Floating layout'
Ui-StopApp
Ui-StartApp 'browse-components-floating' @('--open', $block, '--chrome=floating')
Ui-FocusMain
Libraries-Menu
Ui-Mark
Ui-Step 'Browse Components... (B)'       { Ui-Key 'b' }
Ui-FocusDialog '^Browse Components$'
Ui-ExpectNew 'Library browser view: Installed' 'the browser opened on Installed in the Floating layout'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

Ui-Finish 'UI Windows Browse Components test'
