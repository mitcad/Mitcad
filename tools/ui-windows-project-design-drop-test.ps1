# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files/MainWindowFiles.cpp app/files/MainWindowArrivals.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverOrbitCenter.cpp
# check-all sources: core/model/src/document/link_copies.rs core/model-api/src/components.rs tools/cli
# check-all sources: tools/ui-scripts/project-design-drop.mitcad-ui
# A design of the Projects panel dragged onto the 3D view (mitcad#2428) in
# the build VM, as tools/ui-project-design-drop-test.sh on Linux: the shared
# input script tools/ui-scripts/project-design-drop.mitcad-ui, once in the
# Floating layout (project Shop) and once in the Docked one (project Lab).
# mitcad-cli makes each Local project with assembly.mitcad (a 20 mm cube),
# bracket.mitcad (a block) and back.mitcad (linking assembly.mitcad); the
# app opens assembly.mitcad. The script drags bracket.mitcad onto the cube's
# top face (Linked, at the face's point, one undo step), again while
# Extrude is open (it waits, then Copy), and back.mitcad and assembly.mitcad
# (refused).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-project-design-drop-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-project-design-drop'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then).
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true'; 'projects/panelTopBar' = 'true' }

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the projects are git repositories' }
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

# A Local project of the three designs, back.mitcad made in its folder so
# that its link is assembly.mitcad.
function New-DropProject([string]$Name) {
  $dir = Join-Path $projects $Name
  New-Item -ItemType Directory -Force $dir | Out-Null
  $scripts = Join-Path $PSScriptRoot 'ui-scripts'
  $null = Invoke-Cli @('run', (Join-Path $scripts 'project-design-drop-assembly.json'), '--save',
                       (Join-Path $dir 'assembly.mitcad'))
  $null = Invoke-Cli @('run', (Join-Path $scripts 'project-design-drop-part.json'), '--save',
                       (Join-Path $dir 'bracket.mitcad'))
  Push-Location $dir
  try {
    $null = Invoke-Cli @('run', (Join-Path $scripts 'project-design-drop-back.json'), '--save',
                         (Join-Path $dir 'back.mitcad'))
  } finally {
    Pop-Location
  }
  $null = Invoke-Cli @('project', 'create', $dir, '--author', $author)
  Write-Host "ok   the project $Name is made"
}

# A run of the shared script on the project `Name` in `Layout`.
function Invoke-DropRun([string]$Name, [string]$Layout) {
  $env:MITCAD_TEST_PROJECT = $Name
  $design = Join-Path (Join-Path $projects $Name) 'assembly.mitcad'
  Ui-StartApp "drop-$Layout" @('--ui=qt', '--no-mcp', '--no-recovery', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "$Layout project design drop driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$Layout project design drop driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Layout}: inserted linked and as a copy where dropped, the drop waited for Extrude, links back refused"
  Ui-StopApp
}

New-DropProject 'Shop'
New-DropProject 'Lab'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/project-design-drop.mitcad-ui'

Write-Host '--- 1. Floating: the project Shop'
Invoke-DropRun 'Shop' 'floating'

Write-Host '--- 2. Docked: the project Lab'
Invoke-DropRun 'Lab' 'docked'

Write-Host 'PASS: A design of the Projects panel dropped onto the 3D view'
