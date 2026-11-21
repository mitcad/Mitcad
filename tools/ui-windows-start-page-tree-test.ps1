# SPDX-License-Identifier: MIT
# check-all sources: app/files/StartPage.cpp app/files/StartBrowser.cpp app/MainWindowStart.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: app/projects/ProjectBrowserData.cpp app/projects/ProjectsTree.cpp core/vcs
# The Start page's Tree / Cards toggle (mitcad#2093) in the build VM, as
# tools/ui-start-page-tree-test.sh on Linux: the shared steps
# tools/ui-scripts/start-page-tree*.mitcad-ui on one Local project of the
# projects folder, Shop, with plate, parts/gear and parts/sub/pin, in two
# starts of MitCad with the Start page at launch:
#   1. start-page-tree: Tree shows Shop's folders, choosing parts and then
#      parts/sub in it shows their cards, pin's card opens it; Cards goes
#      back to the breadcrumb view at the same folder; Tree again;
#   2. start-page-tree-again: after the restart Tree at parts/sub; the
#      arrow keys walk and fold the tree, Enter opens gear.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-start-page-tree-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-start-page-tree'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
# A run before left its project there (a project is not made twice).
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
if (Test-Path $work) { Ui-Fail "the work folder of a run before is still there: $work" }
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# The project (as tools\ui-windows-start-page-test.ps1 makes Shop).
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
Write-Host 'ok   the project is made'

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

$env:MITCAD_START_PAGE = '1'
Run-Script 'start-page-tree' 'start-page-tree.mitcad-ui'
Ui-ExpectLog 'Start page tree: Shop | Shop/parts/ | Shop/parts/sub/ (current Shop)' "Tree shows the project's folders"
Ui-ExpectLog 'Start page place: Shop/parts/sub/ (0 folders, 1 designs)' 'the inner folder chosen in the tree'
Ui-ExpectLog 'Start page chosen: open pin.mitcad' 'its card opens its design'
Ui-ExpectLog 'Start page browse view: cards' 'Cards goes back to the breadcrumb view'
Ui-ExpectLog 'Start page crumb Shop/parts/sub/ at ' 'at the same folder'

Run-Script 'start-page-tree-again' 'start-page-tree-again.mitcad-ui'
Ui-ExpectLog 'Start page tree: Shop | Shop/parts/ | Shop/parts/sub/ (current Shop/parts/sub/)' `
  'Tree at the same folder after the restart'
Ui-ExpectLog 'Start page tree folded: Shop/parts/' 'Left folds a folder'
Ui-ExpectLog 'Start page tree unfolded: Shop/parts/' 'Right unfolds it'
Ui-ExpectLog 'Start page chosen: open gear.mitcad' "Enter opens a design from the tree's folder"
Remove-Item Env:MITCAD_START_PAGE -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_PROJECTS_DIR -ErrorAction SilentlyContinue
Ui-StopApp
Write-Host "PASS: the Start page's Tree / Cards toggle: the folders as a tree, their cards, the keyboard and the choice after a restart"
