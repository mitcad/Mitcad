# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files/MainWindowProjects.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverFiles.cpp core/vcs tools/cli tools/ui-scripts/project-files.mitcad-ui
# A project's other files (mitcad#2204) in the build VM, as
# tools/ui-project-files-test.sh on Linux: the shared input script
# tools/ui-scripts/project-files.mitcad-ui, once in the Floating layout
# (project Shop) and once in the Docked one (project Lab). mitcad-cli makes
# each Local project with the design plate.mitcad, a PNG and a PDF recorded
# beside it, and then changes the PDF as its own app would; the script
# checks that the files are listed after the design with their marks and
# tooltip, that a double-click hands the PNG to the system's app (a stub:
# MITCAD_TEST_LOG_URLS, nothing opens), that Record Changes records the
# changed PDF, that Add Files... and a drop from the file manager each copy
# a file in as one version, and that a file is renamed in place and deleted
# as a version.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-project-files-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-project-files'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then).
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true'; 'projects/panelTopBar' = 'true' }

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the versions are checked with it' }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the versions' author.
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
New-Item -ItemType Directory -Force $projects | Out-Null

function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
  return $output
}

# A Local project with plate.mitcad, a PNG and a PDF recorded, the PDF
# changed since; the files to add in its run's outside folder.
function New-FilesProject([string]$Name) {
  $dir = Join-Path $projects $Name
  $run = Join-Path $work $Name
  $null = Invoke-Cli @('project', 'create', $dir, '--author', $author, '--design', 'plate.mitcad')
  [IO.File]::WriteAllText((Join-Path $dir 'photo.png'), 'not really a picture')
  [IO.File]::WriteAllText((Join-Path $dir 'datasheet.pdf'), "%PDF-1.4 not really a datasheet`n")
  $null = Invoke-Cli @('project', 'record', $dir, '--author', $author)
  [IO.File]::AppendAllText((Join-Path $dir 'datasheet.pdf'), "% a page more`n")
  New-Item -ItemType Directory -Force (Join-Path $run 'outside') | Out-Null
  [IO.File]::WriteAllText((Join-Path $run 'outside\notes.txt'), "notes`n")
  [IO.File]::WriteAllText((Join-Path $run 'outside\drop.csv'), "a,b`n1,2`n")
  Write-Host "ok   the project $Name is made"
}

# The versions the script recorded: the files there, the renamed and
# deleted one gone, each change one version.
function Test-Versions([string]$Name) {
  $dir = Join-Path $projects $Name
  $tree = (Invoke-Cli @('project', 'tree', $dir)) -join "`n"
  foreach ($line in '  datasheet.pdf  ', '  drop.csv  ', '  notes.txt  ') {
    if (-not $tree.Contains($line)) { Ui-Fail "${Name}: '$line' is not in the tree: $tree" }
  }
  if ($tree -match 'photo\.png|front\.png|changed|not recorded') { Ui-Fail "${Name}: the tree after the run: $tree" }
  $status = & $git.Source -C $dir status --porcelain
  if ($status) { Ui-Fail "${Name}: git status is not clean: $($status -join ' / ')" }
  $log = & $git.Source -C $dir log --format=%s
  foreach ($message in 'Add notes.txt', 'Add drop.csv', 'Record datasheet.pdf', 'Move photo.png to front.png',
           'Delete front.png') {
    if (-not ($log -contains $message)) { Ui-Fail "${Name}: no version '$message'" }
  }
  Write-Host "ok   ${Name}: each change is one version"
}

# A run of the shared script on the project `Name` in `Layout`.
function Invoke-FilesRun([string]$Name, [string]$Layout) {
  # The script types the paths with forward slashes, as the log shows them.
  $env:MITCAD_TEST_WORK = (Join-Path $work $Name) -replace '\\', '/'
  $env:MITCAD_TEST_PROJECT = $Name
  $env:MITCAD_TEST_PROJECT_DIR = (Join-Path $projects $Name) -replace '\\', '/'
  Ui-StartApp "files-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "$Layout project files driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$Layout project files driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Layout}: listed, opened, recorded, added, dropped, renamed and deleted"
  Ui-StopApp
}

New-FilesProject 'Shop'
New-FilesProject 'Lab'
# The system's app is a stub: nothing opens on the desktop.
$env:MITCAD_TEST_LOG_URLS = '1'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/project-files.mitcad-ui'

Write-Host '--- 1. Floating: the project Shop'
Invoke-FilesRun 'Shop' 'floating'
Test-Versions 'Shop'

Write-Host '--- 2. Docked: the project Lab'
Invoke-FilesRun 'Lab' 'docked'
Test-Versions 'Lab'

Write-Host 'PASS: A project''s other files in the Projects panel'
