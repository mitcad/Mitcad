# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/AppSettings.cpp core/vcs tools/cli
# Names typed in the Projects panel (mitcad#1268) in the build VM, as
# tools/ui-projects-naming-test.sh on Linux: the shared driver scripts
# tools/ui-scripts/projects-naming*.mitcad-ui on a Cloud project of the
# projects folder made with mitcad-cli (a bare repository in a folder stands
# in for the cloud, no network) with Design1 and Design2 recorded and
# Design3.mitcad only on disk; a git of the test's own (a batch file that
# runs Git for Windows) takes some seconds for a project's fetch, so that a
# remote fetch runs while a name is typed:
#   1. Floating: New Design proposes Design4, also while a fetch runs; a
#      design written after the rows were read (Design4, by this script) is
#      not proposed (Design5); a name that is there already keeps the editor
#      with the message below the row and no sheet, and a free name then
#      makes the design; with the Projects card collapsed, New Design
#      expands it, the editor has the focus, a name and Enter make that
#      design, and the card collapses again.
#   2. Docked: the hidden dock is shown for New Design, the proposed name
#      is taken with Enter, and the dock is hidden again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-projects-naming-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-projects-naming'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then); New Design replaces the open design
# (mitcad#1172).
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true'; 'documents/openInCurrentWindow' = 'true' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the project is a Cloud project' }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the versions' author.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n[init]`n`tdefaultBranch = main`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL', 'MITCAD_GIT') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'

$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$shop = Join-Path $projects 'Shop'
$bare = Join-Path $work 'remotes\shop.git'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

Write-Host '--- A Cloud project: Design1 and Design2 recorded, Design3 only on disk'
New-Item -ItemType Directory -Force $projects, (Split-Path $bare) | Out-Null
& $git.Source init -q --bare $bare
if ($LASTEXITCODE -ne 0) { Ui-Fail "git init --bare $bare" }
Invoke-Cli @('project', 'create', $shop, '--author', $author, '--url', $bare)
$design = Join-Path $work 'block.json'
Set-Content -Encoding ascii $design @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 20, "height": 5, "operation": "new_body"}}
]
'@
Invoke-Cli @('run', $design, '--save', (Join-Path $shop 'Design1.mitcad'))
Invoke-Cli @('run', $design, '--save', (Join-Path $shop 'Design2.mitcad'))
Invoke-Cli @('version', 'save', (Join-Path $shop 'Design1.mitcad'), '-m', 'Design1', '--author', $author)
Invoke-Cli @('version', 'save', (Join-Path $shop 'Design2.mitcad'), '-m', 'Design2', '--author', $author)
Invoke-Cli @('push', $shop)
# Not recorded: only the folder on disk has it.
Invoke-Cli @('run', $design, '--save', (Join-Path $shop 'Design3.mitcad'))
Write-Host 'ok   the project is made'

# git for MitCad: a project's fetch (fetch --prune --progress, the remote
# check's) takes some seconds (ping waits without a console's input);
# everything else as Git for Windows does it.
$slowGit = Join-Path $work 'slow-git.cmd'
Set-Content -Encoding ascii $slowGit @"
@echo off
echo %* | findstr /C:"fetch --prune --progress" >nul && ping -n 7 127.0.0.1 >nul
"$($git.Source)" %*
exit /b %ERRORLEVEL%
"@
$env:MITCAD_GIT = $slowGit

$env:MITCAD_TEST_WORK = $work
# Forward slashes, as the app writes paths.
$env:MITCAD_TEST_SHOP = $shop.Replace('\', '/')
$env:MITCAD_TEST_STEP_MS = '100'
$script:Written = $false

# One start of MitCad with a driver script, which ends it.
function Run-Script([string]$Name, [string]$Script, [string]$Chrome) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', "--chrome=$Chrome", '--open', (Join-Path $shop 'Design1.mitcad'))
  $null = $script:UiProcess.Handle
  $clock = [Diagnostics.Stopwatch]::StartNew()
  while (-not $script:UiProcess.HasExited) {
    if ($clock.ElapsedMilliseconds -gt 300000) { Ui-Fail "${Name}: the driver timed out" }
    # A design written after the rows were read: once the second New Design
    # of the Floating steps is cancelled (the script waits for the file).
    if (-not $script:Written) {
      $cancelled = @(Ui-LogLines | Where-Object { $_.TrimEnd() -eq 'Projects panel: new design cancelled' }).Count
      if ($cancelled -ge 2) {
        Copy-Item (Join-Path $shop 'Design1.mitcad') (Join-Path $shop 'Design4.mitcad')
        $script:Written = $true
        Write-Host 'ok   Design4.mitcad written after the rows were read'
      }
    }
    Start-Sleep -Milliseconds 200
  }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: the steps"
}

Write-Host '--- 1. Floating: the proposals, a refused name, the collapsed card'
Run-Script 'naming-floating' 'projects-naming.mitcad-ui' 'floating'
if (-not $script:Written) { Ui-Fail 'Design4.mitcad was not written' }
foreach ($name in 'Plate', 'Gear') {
  if (-not (Test-Path (Join-Path $shop "$name.mitcad"))) { Ui-Fail "$name.mitcad is not in the project" }
}
Write-Host 'ok   Plate and Gear made'

Write-Host '--- 2. Docked: the hidden dock shown for the name and hidden again'
Run-Script 'naming-docked' 'projects-naming-docked.mitcad-ui' 'docked'
if (-not (Test-Path (Join-Path $shop 'Design5.mitcad'))) { Ui-Fail 'Design5.mitcad is not in the project' }
Write-Host 'ok   Design5 made'

Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_GIT -ErrorAction SilentlyContinue
Ui-Finish 'UI Windows Projects naming test'
