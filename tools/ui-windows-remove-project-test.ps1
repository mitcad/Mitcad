# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/TestDriver.cpp app/framework/AppSettings.cpp
# check-all sources: core/vcs core/model/src/file core/ffi/src/vcs.rs core/ffi/src/projects.rs tools/cli
# Remove Project and New Project's Create in a New Subfolder (mitcad#1562)
# in the build VM, as tools/ui-remove-project-test.sh on Linux: the shared
# driver script tools/ui-scripts/remove-project.mitcad-ui on a folder of the
# projects folder with two designs, one in a subfolder, and another file
# (no project):
#   1. New Project on it says that the folder and everything below it become
#      the project, with its 2 designs, and offers Create in a New Subfolder.
#   2. Create in a New Subfolder makes the project in "New Project" inside
#      it.
#   3. Remove Project from that project's row in the Projects panel, after
#      its confirmation, removes the project's files and history; the
#      folders, the designs and the other file stay as they were.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-remove-project-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-remove-project'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then).
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true' } -StepPause 0

$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the author New Project offers.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n[init]`n`tdefaultBranch = main`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL', 'MITCAD_GIT') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}

$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$workshop = Join-Path $projects 'Workshop'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

Write-Host '--- A folder with two designs, one in a subfolder, and notes (no project)'
New-Item -ItemType Directory -Force (Join-Path $workshop 'parts') | Out-Null
$design = Join-Path $work 'block.json'
Set-Content -Encoding ascii $design @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 20, "height": 5, "operation": "new_body"}}
]
'@
$plate = Join-Path $workshop 'plate.mitcad'
$bracket = Join-Path $workshop 'parts\bracket.mitcad'
$notes = Join-Path $workshop 'notes.txt'
Invoke-Cli @('run', $design, '--save', $plate)
Invoke-Cli @('run', $design, '--save', $bracket)
Set-Content -Encoding ascii $notes 'notes'
$plateHash = (Get-FileHash $plate).Hash
$bracketHash = (Get-FileHash $bracket).Hash
Write-Host 'ok   the folder is made'

# Forward slashes, as the app writes paths.
$env:MITCAD_TEST_WORKSHOP = $workshop.Replace('\', '/')
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/remove-project.mitcad-ui'
Ui-StartApp 'remove-project' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
$clock = [Diagnostics.Stopwatch]::StartNew()
while (-not $script:UiProcess.HasExited) {
  if ($clock.ElapsedMilliseconds -gt 240000) { Ui-Fail 'the driver timed out' }
  Start-Sleep -Milliseconds 200
}
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the steps'
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue

Write-Host '--- The project removed, the folders and designs kept'
$made = Join-Path $workshop 'New Project'
if (-not (Test-Path $made -PathType Container)) { Ui-Fail 'the subfolder New Project is gone' }
foreach ($gone in '.git', '.mitcad', '.gitattributes', '.gitignore') {
  if (Test-Path (Join-Path $made $gone)) { Ui-Fail "New Project\$gone is still there" }
  if (Test-Path (Join-Path $workshop $gone)) { Ui-Fail "the folder got $gone" }
}
if ((Get-FileHash $plate).Hash -ne $plateHash) { Ui-Fail 'plate.mitcad changed' }
if ((Get-FileHash $bracket).Hash -ne $bracketHash) { Ui-Fail 'parts\bracket.mitcad changed' }
if ((Get-Content -Raw $notes).Trim() -ne 'notes') { Ui-Fail 'notes.txt changed' }
Write-Host 'ok   only the project''s files are gone'

Ui-Finish 'UI Windows Remove Project test'
