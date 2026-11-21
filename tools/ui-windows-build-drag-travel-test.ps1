# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp app/MainWindow.cpp app/MainWindowJoints.cpp app/framework/TestDriver.cpp core/model/src/joints.rs core/model/src/build core/model-build core/model-api/src/build.rs core/model-api/src/joints.rs tools/cli tools/ui-scripts/build-drag-travel.mitcad-ui tools/ui-scripts/build-drag-travel.json
# Drags leave BUILD's placed parts at their Offset and a carriage slides
# within its travel limits (mitcad#2088, GitHub mitcad/Mitcad#411), in the
# build VM, as tools/ui-build-drag-travel-test.sh on Linux: the shared steps
# tools/ui-scripts/build-drag-travel.mitcad-ui, with a library of a
# synthetic profile, guide rail and carriage made here by mitcad-cli and
# fetched from a local git repository, and the frame build-drag-travel.json
# places from it: a drag of a placed profile is refused in words that say
# to use Placement, Placement moves it and the part on it follows, the
# carriage stops at its limits, and Placement reads and sets its Min and Max.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-build-drag-travel-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-build-drag-travel'),
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

# The library: the parts' designs and the manifest, tagged and fetched.
$library = Join-Path $work 'test-profiles'
Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-profiles', '--name', 'Test profiles', '--license', 'MIT')
New-Item -ItemType Directory -Force (Join-Path $library 'profiles') | Out-Null
New-Item -ItemType Directory -Force (Join-Path $library 'linear') | Out-Null
Run-Cli 'profile' @('run', (Join-Path $PSScriptRoot 'ui-scripts/build-drag-travel-profile.json'), '--save',
                    (Join-Path $library 'profiles\p-drag-test.mitcad'))
Run-Cli 'rail' @('run', (Join-Path $PSScriptRoot 'ui-scripts/build-drag-travel-rail.json'), '--save',
                 (Join-Path $library 'linear\rail-drag-test.mitcad'))
Run-Cli 'carriage' @('run', (Join-Path $PSScriptRoot 'ui-scripts/build-drag-travel-carriage.json'), '--save',
                     (Join-Path $library 'linear\carriage-drag-test.mitcad'))
Copy-Item -Force (Join-Path $PSScriptRoot 'ui-scripts/build-drag-travel-library.json') (Join-Path $library 'mitcad-library.json')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test parts' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)

# The design: the frame, the rail and the carriage.
$designScript = Join-Path $work 'build-drag-travel.json'
$text = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/build-drag-travel.json'))
Set-Content -Encoding ascii $designScript $text.Replace('LIBRARY_URL', $library.Replace('\', '\\'))
$design = Join-Path $work 'build-drag-travel.mitcad'
Run-Cli 'design' @('run', $designScript, '--save', $design)
if (-not (Test-Path $design)) { Ui-Fail 'the frame was not made' }

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/build-drag-travel.mitcad-ui'
Ui-StartApp 'build-drag-travel' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'drag-travel driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "drag-travel driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Drag refused: Arm:1 stays where it was placed: move it with Placement' 'a drag of the arm is refused, naming Placement'
Ui-ExpectLog ': from a, offset 300 mm' 'Placement moved the arm, the post on it following'
Ui-ExpectLog 'Carriage on Rail tx 380 mm' 'the carriage stopped flush with the rail end'
Ui-ExpectLog 'BUILD travel of C' 'Placement read the carriage travel limits'
Ui-ExpectLog 'Carriage on Rail tx 300 mm' 'the carriage stopped at its own Max'
Ui-ExpectLog 'TestDriver: done' 'drags and travel limits of BUILD parts'
Ui-StopApp
Write-Host 'PASS: drags and travel limits of BUILD parts'
