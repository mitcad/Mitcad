# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConvertToFeatures.cpp app/browser/BrowserController.cpp app/MainWindowContextMenu.cpp tools/cli
# check-all sources: core/model/src/document/convert_derived.rs core/model-api/src/convert_derived.rs core/model/src/build core/vcs/src/library
# Convert to Features (mitcad#1815) in the build VM, as
# tools/ui-convert-features-test.sh on Linux: the shared steps
# tools/ui-scripts/convert-features.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/convert-features.json) from a library of one
# synthetic timber member made here (convert-features-member.json and
# convert-features-library.json, fetched from a local git repository):
# Convert to Features from the member's row menu, its sketch and extrusion
# in the timeline, the section's width changed, the joint holding.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-convert-features-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-convert-features'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; the library cannot be fetched, Convert to Features is not tested'
  exit 0
}
function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}
# The library: the member's design and the manifest, committed and fetched.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
$library = Join-Path $work 'test-timber'
Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-timber', '--name', 'Test timber', '--license', 'MIT')
New-Item -ItemType Directory -Force (Join-Path $library 'dimensioned-timber') | Out-Null
Run-Cli 'member' @('run', (Join-Path $PSScriptRoot 'ui-scripts/convert-features-member.json'), '--save',
                   (Join-Path $library 'dimensioned-timber\timber-convert-test.mitcad'))
Copy-Item (Join-Path $PSScriptRoot 'ui-scripts/convert-features-library.json') (Join-Path $library 'mitcad-library.json')
& git -C $library -c user.name=T -c user.email=t@example.invalid add -A 2>&1 | Out-Null
& git -C $library -c user.name=T -c user.email=t@example.invalid commit -q -m 'Test part' 2>&1 | Out-Null
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the library was not committed' }
& git -C $library tag v1.0.0 2>&1 | Out-Null
Run-Cli 'library fetch' @('library', 'fetch', $library)
# The design: the plate and the member joined to it (the library's path
# as library fetch had it, escaped for JSON).
$designScript = Join-Path $work 'convert-features.json'
$text = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'ui-scripts/convert-features.json'))
Set-Content -Encoding ascii $designScript $text.Replace('LIBRARY_URL', $library.Replace('\', '\\'))
$design = Join-Path $work 'convert-features.mitcad'
Run-Cli 'design' @('run', $designScript, '--save', $design)
if (-not (Test-Path $design)) { Ui-Fail 'convert features fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/convert-features.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'convert-features' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'convert features driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "convert features driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the member converted from its row menu, its section edited, the joint holding'
Ui-StopApp
Write-Host 'PASS: Convert to Features keeps the assembly and makes the section editable'
