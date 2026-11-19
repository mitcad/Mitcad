# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DerivedPartCommands.cpp app/MainWindow.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/document/component_options.rs tools/cli
# check-all sources: app/commands/FilletCommands.cpp core/model-api/src/press_pull.rs core/model/src/build
# The edit panel of a derived part (mitcad#1812) in the build VM, as
# tools/ui-derived-part-edit-test.sh on Linux: the shared steps
# tools/ui-scripts/derived-part-edit.mitcad-ui, with a library of one
# synthetic BUILD profile (two rows, Small and Large) made here by
# mitcad-cli and fetched from a local git repository, and a design that
# places it: a double-click on the profile's timeline cell opens Derived
# Component, its length and row change in one undo step and Undo restores
# them; Press Pull on the profile's free end offers Edit <part> Instead,
# which opens Derived Component at its Length, and OK renames the part
# after its new length in the same undo step (mitcad#2030); a double-click
# on a feature type without an edit panel shows the plain message, not a
# failure.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-derived-part-edit-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-derived-part-edit'),
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
Run-Cli 'profile' @('run', (Join-Path $PSScriptRoot 'ui-scripts/derived-part-edit-profile.json'), '--save',
                    (Join-Path $library 'profiles\p-rows-test.mitcad'))
Copy-Item -Force (Join-Path $PSScriptRoot 'ui-scripts/derived-part-edit-library.json') (Join-Path $library 'mitcad-library.json')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)

# The design that places it; the library's path escaped for JSON.
$libraryUrl = $library.Replace('\', '\\')
$designJson = Join-Path $work 'design.json'
$text = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/derived-part-edit.json'))
[System.IO.File]::WriteAllText($designJson, $text.Replace('LIBRARY_URL', $libraryUrl))
$design = Join-Path $work 'derived-part-edit.mitcad'
Run-Cli 'design' @('run', $designJson, '--save', $design)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/derived-part-edit.mitcad-ui'
Ui-StartApp 'derived-part-edit' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'derived part edit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "derived part edit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Derived component C' "the panel changed the profile's length and row with one command"
Ui-ExpectLog 'Press Pull opened DerivedComponent1 for its length' 'Press Pull on the free end opened Derived Component at its Length'
Ui-ExpectLog 'Browser Root/Profile rows test L=700:1 at ' 'the part was named after its new length'
Ui-ExpectLog 'Message band: info This feature has no edit panel yet.' 'a feature without a panel says so in plain words'
Ui-ExpectLog 'Open Read-Only: ' 'Open Source Design opened the library source read-only'
Ui-ExpectLog 'TestDriver: done' 'the panel opened in place, its edit was one undo step and Undo restored it'
Ui-StopApp
Write-Host 'PASS: derived part edit'
