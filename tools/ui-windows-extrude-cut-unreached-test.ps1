# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp geometry/src/sweep.cpp tools/cli
# An Extrude Cut whose preview fails before its cut's targets are found
# (mitcad#2118, GitHub mitcad/Mitcad#492) in the build VM, as
# tools/ui-extrude-cut-unreached-test.sh on Linux: the shared steps
# tools/ui-scripts/extrude-cut-unreached.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/extrude-cut-unreached.json), in a
# 1280 x 600 window, Docked and then Floating: Extrude2 edited, Operation
# New Body and Cut again. The Affected components line last says
# "Affected components could not be found: ..." in words, never
# "Updating affected components..." once the preview has ended, and the
# panel's error box lies within the page's view ("Panel Extrude view y A to
# B"); Docked, the panel is higher than its page.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-cut-unreached-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-cut-unreached'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'extrude-cut-unreached.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-cut-unreached.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-cut-unreached.mitcad-ui'

foreach ($layout in @('docked', 'floating')) {
  Write-Host "--- $layout"
  Ui-StartApp "extrude-cut-unreached-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${layout}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'where the sweep starts, which leaves it no length' "${layout}: the cut's end is reached neither way"
  Ui-ExpectLog 'Cut targets status: Affected components could not be found: ' "${layout}: the list says why in words"
  Ui-ExpectLog 'TestDriver: done' "${layout}: the message is brought into view again"
  # Up to the panel's end (Esc).
  $all = @(Ui-LogLines)
  $lines = @()
  foreach ($line in $all) {
    $lines += $line
    if ($line -match '^Command Extrude cancelled') { break }
  }
  $status = $lines | Where-Object { $_ -match '^Cut targets status: ' } | Select-Object -Last 1
  Write-Host "     $status"
  if (-not ($status -and $status.StartsWith('Cut targets status: Affected components could not be found: '))) {
    Ui-Fail "${layout}: the list's last line is not the failure in words: $status"
  }
  $view = $lines | Where-Object { $_ -match '^Panel Extrude view y -?[0-9]+ to -?[0-9]+, ' } | Select-Object -Last 1
  if (-not ($view -and $view -match '^Panel Extrude view y (-?[0-9]+) to (-?[0-9]+), ')) {
    Ui-Fail "${layout}: the page's view was not logged"
  }
  $top = [int]$Matches[1]
  $bottom = [int]$Matches[2]
  Write-Host "     $view"
  $box = $lines | Where-Object { $_ -match '^Panel Extrude message at ' } | Select-Object -Last 1
  if (-not ($box -and $box -match ' at -?[0-9]+,(-?[0-9]+)')) { Ui-Fail "${layout}: no place logged for the error box" }
  $y = [int]$Matches[1]
  if ($y -lt $top -or $y -gt $bottom) {
    Ui-Fail "${layout}: the error box at y $y, outside the page's view (y $top to $bottom)"
  }
  Write-Host "ok   ${layout}: the error box at y $y, within the page's view (y $top to $bottom)"
  if ($layout -eq 'docked') {
    if (-not ($view -match ', scrolled [0-9]+ of ([0-9]+)') -or [int]$Matches[1] -le 0) {
      Ui-Fail 'docked: the panel is not higher than its page at 1280 x 600 (the test needs one that scrolls)'
    }
    Write-Host "ok   docked: the panel scrolls (by up to $($Matches[1]) px)"
  }
  Ui-StopApp
}
Write-Host 'PASS: an Extrude Cut that fails before its targets says why'
