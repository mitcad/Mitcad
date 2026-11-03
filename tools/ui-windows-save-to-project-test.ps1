# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/framework/CommandRegistry.cpp app/MainWindow.cpp
# check-all sources: core/vcs core/ffi/src/vcs.rs tools/cli
# Input-driven Windows UI test of File > Save to Project (mitcad#503) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted); the
# Linux counterparts are the sections of tools/ui-project-test.sh and
# tools/ui-version-test.sh. A project of the projects folder made with
# mitcad-cli, two loose designs outside it, no network:
#   1. The File menu has Save to Project right after Save As and no Move to
#      a Project. F12 is saved in the settings under the id the command had
#      as Move to a Project (file.move_to_project): it still starts Save to
#      Project. The project picker lists the project; a name typed there
#      saves the loose design in the project's top, as Save As does: the
#      window shows the new file, the file it came from is still there, git
#      (where installed) has the design as a version.
#   2. Save with Description of another loose design asks about the missing
#      version history; its Save to Project button opens the same picker,
#      and the design is saved in the project under its own name.
# Popups such as the File menu close at once in session 0, but the menu's
# entries are logged as it opens.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-save-to-project-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-save-to-project'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The saved shortcut of the command's former id.
Ui-Init $App $Out $QtBin @{ 'file.move_to_project' = 'F12' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$git = Get-Command git -ErrorAction SilentlyContinue
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the versions' author.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'

# The project is in the projects folder, so the picker lists it without
# the app having opened it.
$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$project = Join-Path $projects 'showroom'
$projectLogged = $project -replace '\\', '/'
$loose = Join-Path $work 'loose'
$looseLogged = $loose -replace '\\', '/'
$gear = Join-Path $loose 'gear.mitcad'
$nut = Join-Path $loose 'nut.mitcad'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

function Expect-Exists([string]$Path, [string]$Description) {
  if (-not (Test-Path $Path)) { Ui-Fail "${Description}: $Path is missing" }
  Write-Host "ok   $Description"
}

# The main window's title, waited for.
function Expect-Title([string]$Expected) {
  $title = ''
  for ($i = 0; $i -lt 25; $i++) {
    $title = Ui-WindowTitle
    if ($title -eq $Expected) { Write-Host "ok   title '$Expected'"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "window title is '$title', expected '$Expected'"
}

Write-Host '--- A project and two loose designs'
New-Item -ItemType Directory -Force $loose | Out-Null
$design = Join-Path $work 'block.json'
Set-Content -Encoding ascii $design @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 60, "height": 5, "operation": "new_body"}}
]
'@
Invoke-Cli @('project', 'init', $project, '--author', $author)
Invoke-Cli @('run', $design, '--save', (Join-Path $project 'plate.mitcad'))
Invoke-Cli @('version', 'save', (Join-Path $project 'plate.mitcad'), '-m', 'The plate', '--author', $author)
Invoke-Cli @('run', $design, '--save', $gear)
Invoke-Cli @('run', $design, '--save', $nut)
Write-Host 'ok   the designs are made'

Write-Host '--- 1. File > Save to Project (the old id'"'"'s F12): a name typed in the picker'
Ui-StartApp 'save-to-project' @('--open', $gear)
Ui-ExpectLog "Opened $looseLogged/gear.mitcad" 'the loose design opened'
Ui-ExpectLog 'Project indicator: not in a project' 'in no project'
Ui-Mark
Ui-Step 'the File menu (Alt+F)'          { Ui-Key 'alt+f' }
Ui-ExpectNew 'File menu: ' 'the File menu opened'
$menu = Ui-LastMatch 'File menu: (.*)$'
if ($null -eq $menu -or -not $menu.Contains('Save As | Save to Project | Save with Description')) {
  Ui-Fail "Save to Project is not right after Save As: $menu"
}
if ($menu.Contains('Move to a Project')) { Ui-Fail "the File menu still has Move to a Project: $menu" }
Write-Host 'ok   Save to Project after Save As, no Move to a Project'
Ui-Step 'close the menu (Escape)'        { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Mark
Ui-Step 'save to project (F12)'          { Ui-Key 'F12' }
Ui-ExpectNew 'Project picker Save to Project: a place, reading the projects' 'the saved shortcut of the old id starts it'
Ui-ExpectNew 'Project picker Save to Project: 1 projects: showroom (local)' 'the project listed'
Ui-FocusDialog '^Save to Project$'
Ui-ExpectNew 'Project picker target showroom/gear.mitcad: free' "the design's name offered, free there"
Ui-Step 'a name (the name field has the focus)' { Ui-Type 'valve' }
Ui-ExpectNew 'Project picker target showroom/valve.mitcad: free' 'the name typed'
Ui-Step 'save (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew "Project picker Save to Project: chose $projectLogged/valve.mitcad" 'chosen in the picker'
Ui-FocusMain
Ui-ExpectNew "Saved $projectLogged/valve.mitcad" 'saved in the project' 30
Ui-ExpectNew "Save to Project: $projectLogged/valve.mitcad saved in $projectLogged" 'logged as Save to Project' 30
Ui-ExpectNew "Project: local showroom at $projectLogged" "the window's project is the new file's"
Expect-Title 'valve.mitcad - MitCad'
Expect-Exists (Join-Path $project 'valve.mitcad') 'the design is in the project'
Expect-Exists $gear 'the file it came from is still there'
if ($git) {
  $log = & $git.Source -C $project log -1 --format='%s|%an' 2>&1
  if ("$log" -ne 'Save valve.mitcad|Windows Tester') { Ui-Fail "git log: $log" }
  Write-Host 'ok   the design is a version of the project'
}
Ui-StopApp

Write-Host "--- 2. Save with Description's question opens the picker"
Ui-StartApp 'save-description' @('--open', $nut)
Ui-ExpectLog "Opened $looseLogged/nut.mitcad" 'the other loose design opened'
Ui-Mark
Ui-Step 'save with description (Ctrl+Alt+S)' { Ui-Key 'ctrl+alt+s' }
Ui-ExpectNew 'Save with Description: no version history for nut.mitcad' 'a loose design has no versions'
Ui-Step 'save to project (Enter)'        { Ui-FocusDialog '^Save with Description$'; Ui-Key 'Return' }
Ui-ExpectNew 'Project picker Save to Project: a place, reading the projects' "the question's button opens the picker"
Ui-FocusDialog '^Save to Project$'
Ui-ExpectNew 'Project picker target showroom/nut.mitcad: free' "its own name offered"
Ui-Step 'save (Enter)'                   { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew "Save to Project: $projectLogged/nut.mitcad saved in $projectLogged" 'saved in the project' 30
Expect-Title 'nut.mitcad - MitCad'
Expect-Exists (Join-Path $project 'nut.mitcad') 'the design is in the project'
Expect-Exists $nut 'the file it came from is still there'

Ui-Finish 'UI Windows Save to Project test'
