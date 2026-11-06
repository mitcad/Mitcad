# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/RecentDesigns.cpp app/MainWindowStart.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: app/files/StartBrowser.cpp app/projects/ProjectBrowserData.cpp app/projects/ProjectsTree.cpp core/vcs
# The Start tab (mitcad#1258, GitHub mitcad/Mitcad#125) in the build VM, as
# tools/ui-start-page-test.sh on Linux: the shared steps
# tools/ui-scripts/start-page*.mitcad-ui on three designs that mitcad-cli
# makes (tools/ui-scripts/start-page.json) and two projects of the projects
# folder for the page's project browser (mitcad#1301): Shop (Local) with
# plate, parts/gear and parts/sub/pin, and Team (Cloud through a bare
# repository in a folder when git is installed, else Local; its display
# name Team) with bolts/hex-bolt; in three starts of MitCad:
#   1. start-page-fixture: the three opened, changed and saved (each save
#      writes its preview), no Start page at launch;
#   2. start-page with MITCAD_START_PAGE=1: the Start tab at launch with the
#      three designs and their previews, the star and Pinned, the search,
#      Show start page at launch turned off; the projects: a folder and its
#      sub-folder and back up by the breadcrumb, the search finds Team's
#      design with its project and folder, a project's card opens its
#      design, which joins Recent; a card opens its design, New Design gives
#      an empty design in the default units (inches);
#   3. start-page-off: MitCad starts on an empty design, as before.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-start-page-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-start-page'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
# A run before left its designs and projects there (a project is not made
# twice): a fresh folder, as the Linux and macOS twins' temporary one.
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
if (Test-Path $work) { Ui-Fail "the work folder of a run before is still there: $work" }
New-Item -ItemType Directory -Force $work | Out-Null
foreach ($name in 'a', 'b', 'c') {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/start-page.json') --save (Join-Path $work "start-$name.mitcad")
  if ($LASTEXITCODE -ne 0) { Ui-Fail "start-$name.mitcad was not made" }
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# The two projects (as tools\ui-windows-open-from-project-test.ps1 makes them).
$git = Get-Command git -ErrorAction SilentlyContinue
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n[init]`n`tdefaultBranch = main`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'
$projects = Join-Path $work 'projects'
New-Item -ItemType Directory -Force $projects | Out-Null
$env:MITCAD_PROJECTS_DIR = $projects
$shop = Join-Path $projects 'Shop'
$team = Join-Path $projects 'Team'
$design = Join-Path $PSScriptRoot 'ui-scripts/start-page.json'
function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}
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
  $teamKind = 'cloud'
} else {
  Write-Host 'note: no git: Team is a Local project'
  Invoke-Cli @('project', 'init', $team, '--author', $author)
  $teamKind = 'local'
}
New-Item -ItemType Directory -Force (Join-Path $team 'bolts') | Out-Null
$hex = Join-Path $team 'bolts\hex-bolt.mitcad'
Invoke-Cli @('run', $design, '--save', $hex)
Invoke-Cli @('version', 'save', $hex, '-m', 'Add the bolt', '--author', $author)
# Team's display name on this computer (commands.md, "The display name"):
# written as the file it is kept in, since Windows PowerShell drops the
# quotes of a JSON argument to a program.
$local = Join-Path $team '.mitcad\local'
New-Item -ItemType Directory -Force $local | Out-Null
Set-Content -Encoding ascii (Join-Path $local 'name.json') '{"name": "Team"}'

Write-Host 'ok   the projects are made'

# One start of MitCad with the shared steps; the app ends with them.
function Run-Script([string]$Name, [string]$Script, [string[]]$Arguments = @()) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp $Name (@('--ui=qt', '--no-mcp') + $Arguments)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: the steps"
}

$env:MITCAD_START_PAGE = '0'
Run-Script 'start-page-fixture' 'start-page-fixture.mitcad-ui' @('--open', (Join-Path $work 'start-a.mitcad'))
foreach ($name in 'a', 'b', 'c') {
  Ui-ExpectLog "Design preview saved: start-$name.mitcad (" "start-$name.mitcad saved with its preview"
}

$env:MITCAD_START_PAGE = '1'
Run-Script 'start-page' 'start-page.mitcad-ui'
Ui-ExpectLog 'Start page at launch: shown' 'the Start tab at launch'
Ui-ExpectLog 'Start page recent designs (all): start-c.mitcad (preview) | start-b.mitcad (preview) | start-a.mitcad (preview)' `
  'the three designs with their previews, newest first'
Ui-ExpectLog 'Start page pinned: start-b.mitcad' 'the star pins a design'
Ui-ExpectLog 'Start page recent designs (pinned): start-b.mitcad (preview)' 'Pinned shows the pinned design alone'
Ui-ExpectLog 'Start page search results: 1 (start-a.mitcad)' 'the search finds a recent design'
Ui-ExpectLog "Start page projects: Shop (local) | Team ($teamKind)" 'the projects in the left column'
Ui-ExpectLog 'Start page place: Shop/parts/sub/ (0 folders, 1 designs)' "a project's folder and its sub-folder"
Ui-ExpectLog 'Start page crumb Shop at ' 'the breadcrumb'
Ui-ExpectLog 'Start page search results: 1 (Team/bolts/hex-bolt.mitcad)' 'the search finds a design of the other project'
Ui-ExpectLog 'Start page search result: Team/bolts/hex-bolt.mitcad (Team / bolts)' 'with its project and folder'
Ui-ExpectLog 'Start page chosen: open gear.mitcad' "a project's card opens its design"
Ui-ExpectLog 'Start page recent designs (all): gear.mitcad (no preview) | start-c.mitcad (preview)' `
  "the project's design joined Recent"
Ui-ExpectLog 'Start page chosen: open start-c.mitcad' 'a card opens its design'
Ui-ExpectLog 'Start page at launch: off' 'Show start page at launch turned off'
Ui-ExpectLog 'New design in in' 'New Design: an empty design in the default units'

Run-Script 'start-page-off' 'start-page-off.mitcad-ui'
Ui-ExpectLog 'Start page at launch: off (Show start page at launch)' 'off: no Start page at launch'
Remove-Item Env:MITCAD_START_PAGE -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_PROJECTS_DIR -ErrorAction SilentlyContinue
Ui-StopApp
Write-Host "PASS: the Start tab: recent designs with previews, pins, the projects' browser and its search, New Design and the launch choice"
