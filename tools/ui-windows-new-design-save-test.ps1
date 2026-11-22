# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp app/view/ViewController.cpp
# check-all sources: app/framework/AppSettings.cpp app/framework/TestDriver.cpp core/vcs tools/cli
# check-all sources: tools/ui-scripts/new-design-save.mitcad-ui
# Save of a new design (mitcad#2183) in the build VM, as
# tools/ui-new-design-save-test.sh on Linux: the shared driver script
# tools/ui-scripts/new-design-save.mitcad-ui. The remote project is a Cloud
# project of the projects folder made with mitcad-cli: a local git
# repository following a local bare repository (no network). MitCad starts
# on a new, untitled design:
#   0. Without a remote project (Team hidden from the list): Save As as
#      before, no question.
#   1. Ctrl+S asks On this computer or To a project; Esc saves nothing.
#   2. Save on This Computer: the file dialog, the design saved there.
#   3. A new design, Remember my choice, Save to Project: the project
#      picker, the design saved in the project; the preference remembered.
#   4. The next new design: the picker at once, no question.
#   5. Preferences, General, Save of a new design back to Ask: the
#      question again.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-new-design-save-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-new-design-save'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the remote project is a git repository' }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the versions' author.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n[init]`n`tdefaultBranch = main`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL', 'MITCAD_GIT',
                  'GIT_SSH', 'GIT_ASKPASS') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'

$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$team = Join-Path $projects 'Team'
$bare = Join-Path $work 'remotes\team.git'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

Write-Host '--- A Cloud project: a repository following a local bare repository'
New-Item -ItemType Directory -Force $projects, (Split-Path $bare) | Out-Null
& $git.Source init -q --bare $bare
if ($LASTEXITCODE -ne 0) { Ui-Fail "git init --bare $bare" }
Invoke-Cli @('project', 'create', $team, '--author', $author, '--url', $bare)
$remotes = @(& $git.Source -C $team remote 2>&1)
if ($LASTEXITCODE -ne 0 -or $remotes.Count -eq 0) { Ui-Fail 'the project follows no remote' }
Write-Host 'ok   the remote project is made'

# Forward slashes, as the app writes paths.
$env:MITCAD_TEST_WORK = $work.Replace('\', '/')
$env:MITCAD_TEST_TEAM = $team.Replace('\', '/')
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/new-design-save.mitcad-ui'

Write-Host '--- Save of a new design: the question, both answers, the remembered choice'
Ui-StartApp 'new-design-save' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $script:UiProcess.HasExited) {
  if ($clock.ElapsedMilliseconds -gt 300000) { Ui-Fail 'the driver timed out' }
  Start-Sleep -Milliseconds 200
}
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the steps'

if (-not (Test-Path (Join-Path $work 'local.mitcad'))) { Ui-Fail 'the design saved on this computer is not there' }
if (-not (Test-Path (Join-Path $team 'Untitled.mitcad'))) { Ui-Fail 'the design saved to the project is not there' }
$log = @(& $git.Source -C $team log --format=%s 2>&1)
if (-not ($log | Where-Object { $_ -like '*Untitled.mitcad*' })) { Ui-Fail "no version of Untitled.mitcad: $($log -join ' / ')" }
Write-Host 'ok   one design on this computer, one in the project as a version'

Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Ui-Finish 'UI Windows new design save test'
