# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/build app/commands/BuildCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src/analysis.rs core/vcs/src/library tools/cli
# A member placed with Timber is made of its library's wood (mitcad#1817)
# in the build VM, as tools/ui-timber-material-test.sh on Linux: the shared
# steps tools/ui-scripts/timber-material.mitcad-ui, with a library of one
# synthetic planed 28 x 45 member made here (mitcad-cli,
# tools/ui-scripts/timber-material-member.json; spruce at 450 kg/m3 in its
# build block; no supplier's files) fetched from a local git repository
# (skipped where git is not installed): Timber places it 1800 mm long, and
# Physical Properties shows it as Spruce with the mass its density gives
# (1.0206 kg), never as Steel.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-material-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-material'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: timber material (skipped)'
  exit 0
}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
$library = Join-Path $work 'test-timber-material'

function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}
function Run-Git([string[]]$Arguments) {
  $output = & git -C $library -c user.name=T -c user.email=t@example.invalid @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($Arguments -join ' '): $($output -join ' / ')" }
}

Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-timber-material', '--name', 'Test timber material',
                         '--license', 'MIT')
New-Item -ItemType Directory -Force (Join-Path $library 'planed-timber') | Out-Null
Run-Cli 'run the member' @('run', (Join-Path $PSScriptRoot 'ui-scripts/timber-material-member.json'), '--save',
                          (Join-Path $library 'planed-timber/planed-28x45-test.mitcad'))
Copy-Item (Join-Path $PSScriptRoot 'ui-scripts/timber-material-library.json') (Join-Path $library 'mitcad-library.json')
Run-Git @('add', '-A')
Run-Git @('commit', '-q', '-m', 'Test member')
Run-Git @('tag', 'v1.0.0')
Run-Cli 'library fetch' @('library', 'fetch', $library)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/timber-material.mitcad-ui'
Ui-StartApp 'timber-material' @('--ui=qt', '--no-mcp', '--no-native-dialogs')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'timber material driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "timber material driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Placed Planed 28x45 test' 'a member placed with Timber'
Ui-ExpectLog 'material Spruce' "Physical Properties shows the library's wood"
Ui-ExpectLog 'TestDriver: done' 'the member is Spruce with the mass its density gives, never Steel'
Ui-StopApp
Write-Host 'PASS: timber material'
