# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp app/MainWindow.cpp app/framework/TestDriver.cpp app/icons core/model/src/joints.rs core/model-build core/model-api/src/build.rs tools/cli tools/ui-scripts/build-ground-offset.mitcad-ui tools/ui-scripts/build-face-normal.json
# A BUILD profile placed off a face by Offset and Rotate, both edited later
# with Placement (mitcad#2046, GitHub mitcad/Mitcad#446), in the build VM,
# as tools/ui-build-ground-offset-test.sh on Linux: the shared steps
# tools/ui-scripts/build-ground-offset.mitcad-ui, with the library of one
# synthetic BUILD profile of the mitering test made here by mitcad-cli and
# fetched from a local git repository, and the face-normal test's design (a
# box standing for an imported machine's leg): on its bottom face Offset
# 20 mm and Rotate 30 deg are previewed and placed, then Placement moves the
# profile to Offset 50 mm and Rotate 45 deg in one undo step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-build-ground-offset-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-build-ground-offset'),
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
Run-Cli 'profile' @('run', (Join-Path $PSScriptRoot 'ui-scripts/build-end-meets-profile.json'), '--save',
                    (Join-Path $library 'profiles\p-slot-test.mitcad'))
Copy-Item -Force (Join-Path $PSScriptRoot 'ui-scripts/build-end-meets-library.json') (Join-Path $library 'mitcad-library.json')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)

# The design: the leg.
$design = Join-Path $work 'build-face-normal.mitcad'
Run-Cli 'design' @('run', (Join-Path $PSScriptRoot 'ui-scripts/build-face-normal.json'), '--save', $design)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/build-ground-offset.mitcad-ui'
Ui-StartApp 'build-ground-offset' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'ground-offset driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "ground-offset driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'BUILD face picked: Direction Face normal, at the ' 'a face picked chose Face normal and its centre'
Ui-ExpectLog '; end A at 120.0, 230.0, -20.0; offset 20 mm, rotate 30 deg' 'the preview stands 20 off the face, turned 30 deg'
Ui-ExpectLog ': offset 50 mm, rotate 45 deg' 'Placement moved it to offset 50 mm, rotate 45 deg'
Ui-ExpectLog ': 45 deg to 135 deg' 'Rotate 90 deg turned it a quarter further'
Ui-ExpectLog 'TestDriver: done' 'a profile placed off a face and edited with Placement'
Ui-StopApp
Write-Host 'PASS: a profile off a face by Offset and Rotate'
