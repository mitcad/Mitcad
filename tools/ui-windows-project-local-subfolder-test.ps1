# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/files app/framework/TestDriver.cpp app/framework/AppSettings.cpp
# check-all sources: core/vcs core/model/src/file core/ffi/src/vcs.rs core/ffi/src/projects.rs core/ffi/src/remote.rs
# New Project's Create as Local for Now in a folder that holds a file
# (mitcad#1601) in the build VM, as tools/ui-project-local-subfolder-test.sh
# on Linux: the shared driver script
# tools/ui-scripts/project-local-subfolder.mitcad-ui on a folder of the
# projects folder with one file (no project):
#   1. New Project on the folder offers Create in a New Subfolder, named
#      New Project.
#   2. Cloud with an address that is not reached offers Create as Local for
#      Now.
#   3. Create as Local for Now makes the project in the new subfolder; the
#      folder itself gets none of the project's files and keeps its file.
#
# The test's folders are made under the system's temporary folder, outside
# any git checkout: a folder inside a repository is reported as a project
# "inside" it, which this test does not expect (mitcad#1574).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-project-local-subfolder-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-project-local-subfolder'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0

$work = Join-Path $env:TEMP ('mitcad-ui-project-local-subfolder-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
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
$env:MITCAD_TEST_REMOTE_RETRY_SECONDS = '2'

$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$workshop = Join-Path $projects 'Holds'

Write-Host '--- A folder with a file (no project)'
New-Item -ItemType Directory -Force $workshop | Out-Null
$notes = Join-Path $workshop 'notes.txt'
Set-Content -Encoding ascii $notes 'notes'
Write-Host 'ok   the folder is made'

# Forward slashes, as the app writes paths.
$env:MITCAD_TEST_WORKSHOP = $workshop.Replace('\', '/')
$env:MITCAD_TEST_UNREACHABLE = 'https://127.0.0.1:9/nobody/nothing.git'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/project-local-subfolder.mitcad-ui'
Ui-StartApp 'project-local-subfolder' @('--ui=qt', '--no-mcp')
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

Write-Host '--- The project in the new subfolder, the folder itself no project'
if (-not (Test-Path (Join-Path $workshop 'New Project\.mitcad\project.json') -PathType Leaf)) {
  Ui-Fail 'no project in the new subfolder'
}
foreach ($made in '.mitcad', '.git', '.gitattributes', '.gitignore') {
  if (Test-Path (Join-Path $workshop $made)) { Ui-Fail "the folder with the file got $made" }
}
if ((Get-Content -Raw $notes).Trim() -ne 'notes') { Ui-Fail 'notes.txt changed' }
Write-Host 'ok   the folder with the file is no project'

Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
Ui-Finish 'UI Windows project local subfolder test'
