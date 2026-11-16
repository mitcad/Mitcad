# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandPanel.hpp app/framework/TestDriver.cpp tools/cli
# A new error in a panel higher than its page is scrolled into view by the
# panel itself (mitcad#1884) in the build VM, as
# tools/ui-panel-message-reveal-test.sh on Linux: the shared steps
# tools/ui-scripts/panel-message-reveal.mitcad-ui on the cube that
# mitcad-cli makes (tools/ui-scripts/panel-message-top.json), in a
# 1280 x 600 window, Docked and then Floating: Extrude1 edited, its Distance
# typed 0 fails the preview, and the panel's error box and the Distance
# field typed in lie within the page's view ("Panel Extrude view y A to B")
# without the test scrolling to the box; Docked, the page scrolled to it by
# itself ("Panel Extrude message revealed: ..., scrolled N" with N above 0).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-panel-message-reveal-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-panel-message-reveal'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'panel-message-reveal.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/panel-message-reveal.mitcad-ui'

# The y of the last of $Lines matching $Pattern ("... at x,y"), within the
# page's view [$Top, $Bottom].
function Expect-InView([string]$Layout, [string]$What, [string]$Pattern, [string[]]$Lines, [int]$Top, [int]$Bottom) {
  $line = $Lines | Where-Object { $_ -match $Pattern } | Select-Object -Last 1
  if (-not ($line -and $line -match ' at -?[0-9]+,(-?[0-9]+)')) { Ui-Fail "${Layout}: no place logged for $What" }
  $y = [int]$Matches[1]
  if ($y -lt $Top -or $y -gt $Bottom) {
    Ui-Fail "${Layout}: $What at y $y, outside the page's view (y $Top to $Bottom)"
  }
  Write-Host "ok   ${Layout}: $What at y $y, within the page's view (y $Top to $Bottom)"
}

foreach ($layout in @('docked', 'floating')) {
  Write-Host "--- $layout"
  Ui-StartApp "panel-message-reveal-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${layout}: the panel message driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the panel message driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Preview Extrude: failed' "${layout}: a distance of 0 fails the preview"
  Ui-ExpectLog 'Panel Extrude message box: error: ' "${layout}: its error in the panel's box"
  Ui-ExpectLog 'TestDriver: done' "${layout}: the error is brought into view"
  # From the error to the panel's end (Esc).
  $all = @(Ui-LogLines)
  $from = -1
  for ($i = $all.Count - 1; $i -ge 0; $i--) {
    if ($all[$i] -match '^Panel Extrude message box: error: ') { $from = $i; break }
  }
  if ($from -lt 0) { Ui-Fail "${layout}: no error in the panel's box" }
  $lines = @()
  for ($i = $from; $i -lt $all.Count; $i++) {
    $lines += $all[$i]
    if ($all[$i] -match '^Command Extrude cancelled') { break }
  }
  $view = $lines | Where-Object { $_ -match '^Panel Extrude view y -?[0-9]+ to -?[0-9]+, ' } | Select-Object -Last 1
  if (-not ($view -and $view -match '^Panel Extrude view y (-?[0-9]+) to (-?[0-9]+), ')) {
    Ui-Fail "${layout}: the page's view was not logged after the error"
  }
  $top = [int]$Matches[1]
  $bottom = [int]$Matches[2]
  Write-Host "     $view"
  Expect-InView $layout 'the error box' '^Panel Extrude message at ' $lines $top $bottom
  Expect-InView $layout 'the Distance field typed in' '^Panel Extrude input distance at -?[0-9]+,-?[0-9]+$' $lines $top $bottom
  $revealed = $lines | Where-Object { $_ -match '^Panel Extrude message revealed: ' } | Select-Object -Last 1
  Write-Host "     $revealed"
  if ($layout -eq 'docked') {
    if (-not ($revealed -and $revealed -match ', scrolled ([0-9]+) of [0-9]+$') -or [int]$Matches[1] -le 0) {
      Ui-Fail 'docked: the page did not scroll for the error (the box was in view already: the panel is no longer higher than its page at 1280 x 600)'
    }
    Write-Host "ok   docked: the page scrolled $($Matches[1]) px to the error by itself"
  }
  Ui-StopApp
}
Write-Host "PASS: a panel's new error is scrolled into view"
