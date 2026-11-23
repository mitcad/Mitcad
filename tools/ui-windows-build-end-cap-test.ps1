# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildEndCapCommand.cpp app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/build/BuildLists.cpp app/framework/Selection.cpp app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli tools/ui-scripts/build-end-cap.mitcad-ui tools/ui-scripts/build-end-cap.json
# End Cap (mitcad#2243) in the build VM, as tools/ui-build-end-cap-test.sh
# on Linux: the shared steps tools/ui-scripts/build-end-cap.mitcad-ui, with
# a library of one synthetic steel tube made here by mitcad-cli and fetched
# from a local git repository, and a design of one tube placed by BUILD: a
# click near each end chooses it, both ends are capped outward in one undo
# step, one is switched to Inward (the tube cut back in the cut list), and
# the lists' window shows the plates.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-build-end-cap-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-build-end-cap'),
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

# The library: the tube's design and the manifest, tagged and fetched.
$library = Join-Path $work 'test-tubes'
Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-tubes', '--name', 'Test tubes', '--license', 'MIT')
New-Item -ItemType Directory -Force (Join-Path $library 'tubes') | Out-Null
Run-Cli 'tube' @('run', (Join-Path $PSScriptRoot 'ui-scripts/build-end-cap-profile.json'), '--save',
                 (Join-Path $library 'tubes\tube-test.mitcad'))
Copy-Item -Force (Join-Path $PSScriptRoot 'ui-scripts/build-end-cap-library.json') (Join-Path $library 'mitcad-library.json')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)

# The design that places it; the library's path escaped for JSON.
$libraryUrl = $library.Replace('\', '\\')
$designJson = Join-Path $work 'design.json'
$text = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/build-end-cap.json'))
[System.IO.File]::WriteAllText($designJson, $text.Replace('LIBRARY_URL', $libraryUrl))
$design = Join-Path $work 'build-end-cap.mitcad'
Run-Cli 'design' @('run', $designJson, '--save', $design)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/build-end-cap.mitcad-ui'
Ui-StartApp 'build-end-cap' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'end cap driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "end cap driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'End Cap: 2 cap(s) outward' 'both ends capped outward in one step'
Ui-ExpectLog 'End Cap lists: Tube 40x40x3 L=600 595 mm x1; plates: 2 x Plate 40 x 40 x 5' 'end B switched to inward: the tube cut back, two plates'
Ui-ExpectLog 'Cut list plates: 2 x Plate 40 x 40 x 5' 'the lists show the plates'
Ui-ExpectLog 'TestDriver: done' 'a tube capped at both ends'
Ui-StopApp
Write-Host 'PASS: end caps of a tube'
