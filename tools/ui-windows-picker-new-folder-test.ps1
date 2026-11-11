# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/AppSettings.cpp core/vcs tools/cli
# The project picker's New Folder (mitcad#1649) in the build VM, as
# tools/ui-picker-new-folder-test.sh on Linux: the shared driver script
# tools/ui-scripts/picker-new-folder.mitcad-ui on a Local project of the
# projects folder made with mitcad-cli (a recorded design) and a loose
# design in no project, opened:
#   1. Save to Project: the project selected, New Folder, a name typed and
#      Enter make the folder as a version, selected in the picker.
#   2. The same name again: the refusal in words below the name, the editor
#      open again; Esc takes the row away.
#   3. Save: the design is in the new folder, and the Projects panel shows
#      the folder and the design.
#   4. Save As's picker: a folder made with the name proposed, then the
#      picker cancelled: the folder stays and the panel shows it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-picker-new-folder-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-picker-new-folder'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then).
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true' } -StepPause 0

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the folders are versions' }
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
$loose = Join-Path $work 'loose'
$gear = Join-Path $loose 'gear.mitcad'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

Write-Host '--- A Local project with plate.mitcad recorded, a loose design'
New-Item -ItemType Directory -Force $projects, $loose | Out-Null
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
Write-Host 'ok   the designs are made'

$env:MITCAD_TEST_WORK = $work
# Forward slashes, as the app writes paths.
$env:MITCAD_TEST_SHOP = $shop.Replace('\', '/')
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/picker-new-folder.mitcad-ui'

Write-Host '--- New Folder in the project picker'
Ui-StartApp 'picker-new-folder' @('--ui=qt', '--no-mcp', '--open', $gear)
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

foreach ($folder in 'Brackets', 'New Folder') {
  if (-not (Test-Path -PathType Container (Join-Path $shop $folder))) { Ui-Fail "the folder $folder is not in the project" }
}
if (-not (Test-Path (Join-Path $shop 'Brackets\gear.mitcad'))) { Ui-Fail 'gear.mitcad is not in the new folder' }
if (-not (Test-Path $gear)) { Ui-Fail 'the file the design came from is gone' }
Write-Host 'ok   the folders and the design are in the project'
# Each folder a version of its own.
$log = @(& $git.Source -C $shop log --format=%s 2>&1)
foreach ($subject in 'Add folder Brackets', 'Add folder New Folder') {
  if (-not ($log -contains $subject)) { Ui-Fail "no version '$subject': $($log -join ' / ')" }
}
Write-Host 'ok   the folders are versions'

Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Ui-Finish 'UI Windows project picker New Folder test'
