# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: core/vcs tools/cli tools/ui-scripts/slow-remote.mitcad-ui tools/ui-scripts/slow-ssh.sh
# A Cloud project with a slow server (mitcad#1895) in the build VM, as
# tools/ui-slow-remote-test.sh on Linux: the shared driver script
# tools/ui-scripts/slow-remote.mitcad-ui on a Cloud project of the projects
# folder made with mitcad-cli, its server a local bare repository behind an
# ssh that waits 5 s per connection (tools/ui-scripts/slow-ssh.sh, run by
# Git for Windows' sh), edit locks on, and a loose design opened:
#   1. Save to Project's New Folder: the folder is a version at once, sent
#      in the background.
#   2. Save in the new folder: saved and recorded at once, the window takes
#      a change while the version, the edit lock and the folder's version go
#      to the server one after another.
#   3. Save again: recorded at once, sent in the background, the state in
#      words (sending, then up to date with the last sync's time).
# The UI thread never stalls meanwhile ("UI stall: " lines, logged with
# MITCAD_TEST_UI_STALL_MS), and the server has every version at the end.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-slow-remote-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-slow-remote'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on.
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the project is shared through git' }
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
$shop = Join-Path $projects 'Shop'
$loose = Join-Path $work 'loose'
$gear = Join-Path $loose 'gear.mitcad'
$server = Join-Path $work 'server'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

function Invoke-Git([string[]]$Arguments) {
  $output = & $git.Source @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($Arguments -join ' '): $($output -join ' / ')" }
  return $output
}

Write-Host '--- A Cloud project with plate.mitcad, its server slow; a loose design'
New-Item -ItemType Directory -Force $projects, $loose, $server | Out-Null
Invoke-Cli @('project', 'init', $shop, '--author', $author)
$design = Join-Path $work 'block.json'
Set-Content -Encoding ascii $design @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 20, "height": 5, "operation": "new_body"}}
]
'@
Invoke-Cli @('run', $design, '--save', (Join-Path $shop 'plate.mitcad'))
Invoke-Cli @('version', 'save', (Join-Path $shop 'plate.mitcad'), '-m', 'The plate', '--author', $author)
Invoke-Cli @('run', $design, '--save', $gear)
# The server: a bare repository reached through an ssh that waits. git
# runs the ssh command with its own sh; paths with forward slashes.
$null = Invoke-Git @('init', '-q', '--bare', (Join-Path $server 'shop.git'))
$sshScript = (Join-Path $PSScriptRoot 'ui-scripts/slow-ssh.sh').Replace('\', '/')
$env:GIT_SSH_COMMAND = "sh `"$sshScript`""
$env:GIT_SSH_VARIANT = 'ssh'
$env:SLOW_SSH_ROOT = $server.Replace('\', '/')
$env:SLOW_SSH_SECONDS = '0'
$null = Invoke-Git @('-C', $shop, 'remote', 'add', 'origin', 'ssh://test/shop.git')
$null = Invoke-Git @('-C', $shop, 'push', '-q', '-u', 'origin', 'main')
Write-Host 'ok   the project is shared'

# Every connection waits 5 s; a gap of the UI thread over 4 s is logged.
$env:SLOW_SSH_SECONDS = '5'
$env:MITCAD_TEST_UI_STALL_MS = '4000'
$env:MITCAD_TEST_REMOTE_RETRY_SECONDS = '2'
$env:MITCAD_TEST_WORK = $work
# Forward slashes, as the app writes paths.
$env:MITCAD_TEST_SHOP = $shop.Replace('\', '/')
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/slow-remote.mitcad-ui'

Write-Host '--- New Folder, Save to Project and Save while the server is slow'
Ui-StartApp 'slow-remote' @('--ui=qt', '--no-mcp', '--open', $gear)
$null = $script:UiProcess.Handle
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $script:UiProcess.HasExited) {
  if ($clock.ElapsedMilliseconds -gt 600000) { Ui-Fail 'the driver timed out' }
  Start-Sleep -Milliseconds 200
}
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the steps'
Ui-ExpectLog 'UI stall watch: gaps over 4000 ms' "the UI thread's stalls are watched"

# From the first step on, the UI thread never waited for the server.
$lines = @(Ui-LogLines)
$from = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
  if ($lines[$i] -like 'Project picker Save to Project: *') { $from = $i; break }
}
if ($from -lt 0) { Ui-Fail 'the project picker never listed the projects' }
$stall = $lines[$from..($lines.Count - 1)] | Where-Object { $_ -like 'UI stall: *' } | Select-Object -First 1
if ($stall) { Ui-Fail "the window stopped responding while the server was slow: $stall" }
Write-Host 'ok   the window kept responding'

# The server has the folder and both versions of the design.
$serverGit = Join-Path $server 'shop.git'
$sent = @(Invoke-Git @('--git-dir', $serverGit, 'log', '--format=%s', 'main'))
if (-not ($sent -contains 'Add folder Parts')) { Ui-Fail "the folder's version is not on the server: $($sent -join ' / ')" }
$versions = @(Invoke-Git @('--git-dir', $serverGit, 'log', '--format=%H', 'main', '--', 'Parts/gear.mitcad'))
if ($versions.Count -lt 2) { Ui-Fail "the server has $($versions.Count) version(s) of Parts/gear.mitcad, not 2" }
Write-Host 'ok   the server has every version'

foreach ($name in 'MITCAD_TEST_INPUT', 'GIT_SSH_COMMAND', 'GIT_SSH_VARIANT', 'SLOW_SSH_ROOT', 'SLOW_SSH_SECONDS',
                  'MITCAD_TEST_UI_STALL_MS') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
Ui-Finish 'UI Windows slow remote test'
