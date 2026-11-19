# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli tools/ui-scripts/build-lines.mitcad-ui tools/ui-scripts/build-lines.json
# Profile along sketch lines (mitcad#1965) in the build VM, as
# tools/ui-build-lines-test.sh on Linux: the shared steps
# tools/ui-scripts/build-lines.mitcad-ui, with a library of one synthetic
# BUILD profile made here by mitcad-cli and fetched from a local git
# repository, and a design of a sketch of four lines in a rectangle:
# Profile, Method Along sketch lines, the sketch picked whole from the
# Browser, Place: four profiles mitered at the corners in one undo step,
# following a line moved in the sketch.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-build-lines-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-build-lines'),
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

# The design that places it; the library's path escaped for JSON.
$libraryUrl = $library.Replace('\', '\\')
$designJson = Join-Path $work 'design.json'
$text = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/build-lines.json'))
[System.IO.File]::WriteAllText($designJson, $text.Replace('LIBRARY_URL', $libraryUrl))
$design = Join-Path $work 'build-lines.mitcad'
Run-Cli 'design' @('run', $designJson, '--save', $design)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/build-lines.mitcad-ui'
Ui-StartApp 'build-lines' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'along sketch lines driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "along sketch lines driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'BUILD profile along sketch lines: ' 'the sketch picked from the Browser'
Ui-ExpectLog 'profile(s) along sketch lines' 'Place made the profiles along the lines'
Ui-ExpectLog 'TestDriver: done' 'profiles along sketch lines placed and following the sketch'
Ui-StopApp
Write-Host 'PASS: profile along sketch lines'
