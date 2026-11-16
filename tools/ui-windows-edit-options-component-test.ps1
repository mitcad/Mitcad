# SPDX-License-Identifier: MIT
# check-all sources: app/library/EditOptionsDialog.cpp app/library/PartOptions.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/component_options.rs tools/cli
# Edit Options of an inserted library part (mitcad#1929) in the build VM,
# as tools/ui-edit-options-component-test.sh on Linux: the shared steps
# tools/ui-scripts/edit-options-component.mitcad-ui, with a library of one
# synthetic plate with options (Color, Width) made here by mitcad-cli and
# fetched from a local git repository, and a design that inserts it: Width
# changed in Edit Options is previewed without a refusal, and OK keeps it
# in one undo step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-edit-options-component-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-edit-options-component'),
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

# The library: the plate with its options, tagged and fetched.
$library = Join-Path $work 'test-options'
Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-options', '--name', 'Test options', '--license', 'MIT')
$plate = Join-Path $work 'plate.mitcad'
Run-Cli 'plate' @('run', (Join-Path $PSScriptRoot 'ui-scripts/edit-options-component-plate.json'), '--save', $plate)
Run-Cli 'library add' @('library', 'add', $library, $plate, '--id', 'plate', '--name', 'Plate', '--designation', 'Plate',
                        '--category', 'plates')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)

# The design that inserts it; the library's path escaped for JSON.
$libraryUrl = $library.Replace('\', '\\')
$designJson = Join-Path $work 'design.json'
$text = [System.IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/edit-options-component.json'))
[System.IO.File]::WriteAllText($designJson, $text.Replace('LIBRARY_URL', $libraryUrl))
$design = Join-Path $work 'edit-options-component.mitcad'
Run-Cli 'design' @('run', $designJson, '--save', $design)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/edit-options-component.mitcad-ui'
Ui-StartApp 'edit-options-component' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Edit Options driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Edit Options driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Edit Options option Width chosen: 40 mm' 'Width changed in Edit Options'
Ui-ExpectLog 'Edit Options of Plate: ' 'OK kept the options with one command'
Ui-ExpectLog 'TestDriver: done' 'the preview took the value and OK changed the plate in one undo step'
Ui-StopApp
Write-Host 'PASS: Edit Options of a component'
