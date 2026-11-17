# SPDX-License-Identifier: MIT
# Screenshots of Flex (mitcad#1713) in the build VM, for a reviewer to
# judge, in the Docked and the Floating layout: not a test (a shot fails
# only when the steps that lead to it do). The in-process test driver
# (MITCAD_TEST_INPUT, app/framework/TestDriver.hpp) runs
# tools/ui-scripts/flex-shots-lay.mitcad-ui on tools/ui-scripts/flex.json
# (laying an item: the command opened, a pipe's end port, the route under
# the pointer, the hose added, a warning) and
# tools/ui-scripts/flex-shots-edit.mitcad-ui on
# tools/ui-scripts/flex-shots-edit.json (a flat cable's half twist, an item
# selected with its handles, a node dragged, the twist ring); `screenshot`
# grabs the whole main window with the view and the panels over it. The
# files are <Out>\NN-<layout>-<what><Suffix>.png.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-flex-shots.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe -Out <dir> [-Suffix -after] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-flex-shots'),
  [string]$Suffix = '',
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:FLEX_SHOTS = $script:UiOut
$env:FLEX_END = "$Suffix.png"
$failed = 0
foreach ($chrome in @('docked', 'floating')) {
  $env:FLEX_LAYOUT = $chrome
  foreach ($run in @(@('lay', 'flex.json'), @('edit', 'flex-shots-edit.json'))) {
    $name = $run[0]
    Write-Host "--- Flex $name shots in the $chrome layout"
    $design = Join-Path $work "flex-$name-$chrome.mitcad"
    Remove-Item -Force $design -ErrorAction SilentlyContinue
    & $Cli run (Join-Path $PSScriptRoot "ui-scripts/$($run[1])") --save $design | Out-Null
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Write-Host "FAIL: the $name design was not made"; $failed++; continue }
    $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/flex-shots-$name.mitcad-ui"
    Ui-StartApp "flex-$name-$chrome" @('--ui=qt', '--no-mcp', "--chrome=$chrome", '--open', $design)
    $null = $script:UiProcess.Handle
    if (-not $script:UiProcess.WaitForExit(420000)) { Write-Host "FAIL: the $name shots timed out ($chrome)"; $failed++ }
    elseif (-not ((Ui-LogText) -match 'TestDriver: done')) { Write-Host "FAIL: the $name shots stopped early ($chrome)"; $failed++ }
    Ui-StopApp
  }
}
Get-ChildItem $script:UiOut -Filter '*.png' | ForEach-Object { Write-Host "shot $($_.Name)" }
if ($failed -gt 0) { exit 1 }
Write-Host 'DONE: Flex shots'
