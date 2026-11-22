# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowJoints.cpp tools/cli
# check-all sources: core/model/src/document/link_tree.rs core/model/src/document/flexible.rs
# check-all sources: tools/ui-scripts/linked-flexible.mitcad-ui
# A design linked with its components made Flexible (mitcad#2209) in the
# build VM, as tools/ui-linked-flexible-test.sh on Linux: the shared steps
# tools/ui-scripts/linked-flexible.mitcad-ui on the designs mitcad-cli makes
# in the work folder (tools/ui-scripts/linked-flexible-source.json and
# linked-flexible-holder.json, which links the first whole). Flexible is
# offered on the linked sub-assembly's Browser row, and a drag moves the
# Key along the design's own slider for that occurrence alone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-linked-flexible-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-linked-flexible'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$source = Join-Path $work 'linked-flexible-source.mitcad'
$holder = Join-Path $work 'holder.mitcad'
# The holder's script with the linked design's path (forward slashes).
$holderScript = Join-Path $work 'linked-flexible-holder.json'
$steps = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/linked-flexible-holder.json')
Set-Content -Encoding ascii $holderScript ($steps.Replace('@SOURCE@', ($source -replace '\\', '/')))
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/linked-flexible-source.json') --save $source 2>&1
$status = $LASTEXITCODE
if ($status -eq 0) {
  $output = & $Cli run $holderScript --save $holder 2>&1
  $status = $LASTEXITCODE
}
if ($status -ne 0 -or -not (Test-Path $holder)) { Ui-Fail "mitcad-cli: the designs were not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/linked-flexible.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'linked-flexible' @('--ui=qt', '--no-mcp', '--open', $holder)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'linked flexible driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "linked flexible driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Slider:1 made flexible, its Key dragged along the design''s slider alone'
Ui-StopApp
Write-Host 'PASS: a design linked with its components is flexible and its slider part drags'
