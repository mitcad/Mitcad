# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/TestDriver.cpp tools/cli core/ffi/src/serp_test.rs
# check-all sources: core/serp core/model/src/exchange core/model/src/exchange.rs core/model-api/src/exchange.rs
# check-all sources: tools/ui-scripts/serp.mitcad-ui
# .serp files (mitcad#1870) in the build VM, as tools/ui-serp-test.sh on
# Linux: the shared input script tools/ui-scripts/serp.mitcad-ui on the
# import's test file written by mitcad-cli serp-test-file: --open of it
# makes a new document of its five bodies, the block a component placed
# twice; File > Import of it into that design asks where it goes and makes
# a component named after the file; one undo step takes it back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-serp-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-serp'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$sample = Join-Path $work 'sample.serp'
& $Cli serp-test-file $sample
if ($LASTEXITCODE -ne 0) { Ui-Fail '.serp test file creation failed' }

# The script types the path with forward slashes, as the log shows them.
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/serp.mitcad-ui'
Ui-StartApp 'serp' @('--ui=qt', '--no-mcp', '--open', $sample)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail '.serp open and import driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail ".serp open and import driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'File > Open of the .serp: its bodies and the block placed twice; File > Import asks where it goes'
Ui-StopApp
Write-Host 'PASS: .serp open and import'
