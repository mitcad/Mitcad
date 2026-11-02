# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files/MainWindowProjects.cpp app/files/MainWindowFiles.cpp
# check-all sources: app/framework/AppSettings.cpp core/vcs tools/cli
# check-all sources: app/browser/DesignPropertiesDialog.cpp app/browser/BrowserController.cpp
# Input-driven Windows UI test of File > Open from Project (mitcad#530) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted); the
# Linux counterpart is tools/ui-open-from-project-test.sh. Two projects of
# the projects folder made with mitcad-cli: Shop (Local) with a design in
# its top, a folder with a design and a sub-folder with another; Team with
# a design in a folder (Cloud, through a bare repository in a folder, when
# git is installed; else Local). Keys only: popups cannot open in session 0.
#   1. The File menu has Open from Project after Open, before Open Recent.
#      Its shortcut (F12, the test's own) lists both projects and starts in
#      the current project's top; Enter goes into a folder and its
#      sub-folder, Backspace goes up.
#   2. The search finds a design of the other project by part of its name
#      as it is typed; Enter opens it.
#   3. The next time it starts in that design's folder; with Read-only
#      checked the search's design opens read-only, where Design
#      Properties (F11, the test's own) opens read-only and its Statistics
#      tab reads (mitcad#1149).
#   4. After a restart it starts in the place shown last; Escape cancels.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-open-from-project-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-open-from-project'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The commands' shortcuts of the test's own.
Ui-Init $App $Out $QtBin @{ 'file.open_from_project' = 'F12'; 'file.design_properties' = 'F11' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$git = Get-Command git -ErrorAction SilentlyContinue
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n[init]`n`tdefaultBranch = main`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'

$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$shop = Join-Path $projects 'Shop'
$team = Join-Path $projects 'Team'
$shopLogged = $shop -replace '\\', '/'
$teamLogged = $team -replace '\\', '/'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

# Open-Browser: File > Open from Project by its shortcut; both projects listed.
function Open-Browser {
  Ui-FocusMain
  Ui-Mark
  Ui-Step 'open from project (F12)'      { Ui-Key 'F12' }
  Ui-ExpectNew 'Project browser Open from Project: reading the projects' 'the window opened' 20
  Ui-ExpectNew 'Project browser Open from Project: 2 projects: ' 'both projects listed' 20
  $listed = Ui-LastMatch 'Project browser Open from Project: 2 projects: (.*)$'
  if (-not $listed.Contains('Shop (local)') -or -not $listed.Contains($script:teamKind)) {
    Ui-Fail "the projects listed: $listed"
  }
  Ui-FocusDialog '^Open from Project$'
}

Write-Host '--- Two projects with folders and designs'
$design = Join-Path $work 'block.json'
Set-Content -Encoding ascii $design @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 60, "height": 5, "operation": "new_body"}}
]
'@
Invoke-Cli @('project', 'init', $shop, '--author', $author)
New-Item -ItemType Directory -Force (Join-Path $shop 'parts\sub') | Out-Null
foreach ($name in 'plate', 'parts\gear', 'parts\sub\pin') {
  $file = Join-Path $shop "$name.mitcad"
  Invoke-Cli @('run', $design, '--save', $file)
  Invoke-Cli @('version', 'save', $file, '-m', "Add $name", '--author', $author)
}
if ($git) {
  $bare = Join-Path $work 'remotes\team.git'
  New-Item -ItemType Directory -Force (Split-Path $bare) | Out-Null
  & $git.Source init -q --bare $bare
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git init --bare $bare" }
  Invoke-Cli @('project', 'create', $team, '--author', $author, '--url', $bare)
  $script:teamKind = '(cloud)'
} else {
  Write-Host 'note: no git: Team is a Local project'
  Invoke-Cli @('project', 'init', $team, '--author', $author)
  $script:teamKind = 'Team (local)'
}
New-Item -ItemType Directory -Force (Join-Path $team 'bolts') | Out-Null
$hex = Join-Path $team 'bolts\hex-bolt.mitcad'
Invoke-Cli @('run', $design, '--save', $hex)
Invoke-Cli @('version', 'save', $hex, '-m', 'Add the bolt', '--author', $author)
Write-Host 'ok   the projects are made'

Write-Host '--- 1. The File menu; the projects, a folder and its sub-folder'
Ui-StartApp 'open-from-project' @('--open', (Join-Path $shop 'plate.mitcad'))
Ui-ExpectLog "Opened $shopLogged/plate.mitcad" 'a design of Shop is open' 20
Ui-Mark
Ui-Step 'the File menu (Alt+F)'          { Ui-Key 'alt+f' }
Ui-ExpectNew 'File menu: ' 'the File menu opened'
$menu = Ui-LastMatch 'File menu: (.*)$'
if ($null -eq $menu -or -not $menu.Contains('Open | Open from Project | Open Recent >')) {
  Ui-Fail "Open from Project is not between Open and Open Recent: $menu"
}
Write-Host 'ok   Open from Project after Open, before Open Recent'
Ui-Step 'close the menu (Escape)'        { Ui-Key 'Escape' }
Open-Browser
Ui-ExpectNew 'Project browser starts at Shop (current project)' 'it starts in the current project'
Ui-ExpectNew 'Project browser shows Shop: parts/ | plate.mitcad v1' 'the top: the folder first, then the design'
Ui-Mark
Ui-Step 'into the folder (Enter)'        { Ui-Key 'Return' }
Ui-ExpectNew 'Project browser shows Shop/parts/: sub/ | gear.mitcad v1' 'the folder: its sub-folder, its design'
Ui-Mark
Ui-Step 'into the sub-folder (Enter)'    { Ui-Key 'Return' }
Ui-ExpectNew 'Project browser shows Shop/parts/sub/: pin.mitcad v1' 'the sub-folder'
Ui-Mark
Ui-Step 'up (Backspace)'                 { Ui-Key 'BackSpace' }
Ui-ExpectNew 'Project browser shows Shop/parts/: sub/ | gear.mitcad v1' 'Backspace goes up'

Write-Host '--- 2. The search finds a design of the other project; Enter opens it'
Ui-Step 'the search (Alt+S)'             { Ui-Key 'alt+s' }
Ui-Mark
Ui-Step 'type a part of its name'        { Ui-Type 'hex' }
Ui-ExpectNew 'Project browser search "hex" in all projects: 1 found: Team/bolts/hex-bolt.mitcad' 'found as it is typed'
Ui-Mark
Ui-Step 'open it (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew "Project browser Open from Project: chose $teamLogged/bolts/hex-bolt.mitcad" 'chosen'
Ui-FocusMain
Ui-ExpectNew "Opened $teamLogged/bolts/hex-bolt.mitcad" "the other project's design opened" 20

Write-Host '--- 3. The place remembered; Read-only'
Open-Browser
Ui-ExpectNew 'Project browser starts at Team/bolts/ (remembered)' "it starts in the design's folder"
Ui-ExpectNew 'Project browser shows Team/bolts/: hex-bolt.mitcad v1' "the folder's design"
Ui-Step 'the search (Alt+S)'             { Ui-Key 'alt+s' }
Ui-Mark
Ui-Step 'type a part of its name'        { Ui-Type 'gear' }
Ui-ExpectNew 'Project browser search "gear" in all projects: 1 found: Shop/parts/gear.mitcad' 'found'
Ui-Mark
Ui-Step 'read-only (Alt+R)'              { Ui-Key 'alt+r' }
Ui-ExpectNew 'Project browser read-only: on' 'Read-only checked'
Ui-Step 'back to the search (Alt+S)'     { Ui-Key 'alt+s' }
Ui-Mark
Ui-Step 'open it (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew "Project browser Open from Project: chose $shopLogged/parts/gear.mitcad (read-only)" 'chosen read-only'
Ui-FocusMain
Ui-ExpectNew "Open Read-Only: $shopLogged/parts/gear.mitcad" 'opened read-only' 20
# Design Properties in the read-only window (mitcad#1149): the summary
# without editing, the Statistics tab read.
Ui-Mark
Ui-Step 'Design Properties (F11)'        { Ui-Key 'F11' }
Ui-ExpectNew 'Design Properties opened read-only: Created ' 'opened read-only, with its statistics'
Ui-FocusDialog '^Design Properties$'
Ui-Step 'the Statistics tab (Ctrl+Tab)'  { Ui-Key 'ctrl+Tab' }
Ui-ExpectNew 'Design Properties tab: Statistics (Created ' 'the Statistics tab reads'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }
Ui-ExpectNew 'Design Properties closed' 'closed, nothing to apply'
Ui-FocusMain
Ui-StopApp

Write-Host '--- 4. After a restart: the place shown last; Escape'
Ui-StartApp 'open-from-project-again'
Open-Browser
Ui-ExpectNew 'Project browser starts at Shop/parts/ (remembered)' 'the place shown last'
Ui-ExpectNew 'Project browser shows Shop/parts/: sub/ | gear.mitcad v1' 'its contents'
Ui-Mark
Ui-Step 'cancel (Escape)'                { Ui-Key 'Escape' }
Ui-ExpectNew 'Project browser Open from Project: cancelled' 'cancelled'

Ui-Finish 'UI Windows Open from Project test'
