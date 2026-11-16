# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/commands/BuildCommands.cpp app/framework/Selection.cpp app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli tools/ui-scripts/joint-placement-edit.mitcad-ui tools/ui-scripts/joint-placement-edit.json
# A BUILD part's joint and placement edited (mitcad#1885, GitHub
# mitcad/Mitcad#354) in the build VM, as
# tools/ui-joint-placement-edit-test.sh on Linux: the shared steps
# tools/ui-scripts/joint-placement-edit.mitcad-ui, with a library of one
# synthetic BUILD profile with a slot on one side made here by mitcad-cli
# and fetched from a local git repository, and a design that places it: a
# joint onto a profile's mount frame with a frame override opens in Joint,
# its Offset changed keeps the override; Placement turns a side
# placement's From from Middle to End A keeping the part where it is, then
# places it by a new Offset, each in one undo step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-joint-placement-edit-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-joint-placement-edit'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is not installed: the test library cannot be fetched' }
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'

# The library: the profile's design and the manifest, tagged and fetched.
$library = Join-Path $work 'test-profiles'
Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-profiles', '--name', 'Test profiles', '--license', 'MIT')
New-Item -ItemType Directory -Force (Join-Path $library 'profiles') | Out-Null
Run-Cli 'profile' @('run', (Join-Path $PSScriptRoot 'ui-scripts/joint-placement-edit-profile.json'), '--save',
                    (Join-Path $library 'profiles\p-slot-test.mitcad'))
Copy-Item -Force (Join-Path $PSScriptRoot 'ui-scripts/joint-placement-edit-library.json') (Join-Path $library 'mitcad-library.json')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)

# The design that places it; the library's path escaped for JSON.
$libraryUrl = $library.Replace('\', '\\')
$designJson = Join-Path $work 'design.json'
$text = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/joint-placement-edit.json'))
[System.IO.File]::WriteAllText($designJson, $text.Replace('LIBRARY_URL', $libraryUrl))
$design = Join-Path $work 'joint-placement-edit.mitcad'
Run-Cli 'design' @('run', $designJson, '--save', $design)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/joint-placement-edit.mitcad-ui'
Ui-StartApp 'joint-placement-edit' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'joint placement edit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "joint placement edit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Joint origin b keeps its frame override' 'the joint on the mount frame opened in Joint with its override'
Ui-ExpectLog 'Edited F3' 'the new Offset of the joint committed'
Ui-ExpectLog 'Placement of C3: from a, kept in place' 'a new From kept the part where it is'
Ui-ExpectLog 'Placement of C3: from a, offset 50 mm' 'a new Offset placed the part from End A'
Ui-ExpectLog 'TestDriver: done' 'the joint and the placement of a BUILD part edited'
Ui-StopApp
Write-Host 'PASS: joint and placement edit'
