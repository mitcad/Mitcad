# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/RibbonFit.cpp app/framework/RibbonLabels.cpp app/resources/layout/default.json app/drawings/MainWindowDrawings.cpp app/drawings/DrawingWorkspace.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/drawing-ribbon-fit.mitcad-ui tools/ui-scripts/drawing-ribbon-fit.json
# The DRAWING tab's groups at 1280 px (mitcad#2066) in the build VM, as
# tools/ui-drawing-ribbon-fit-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-ribbon-fit.mitcad-ui on a box with one drawing
# (tools/ui-scripts/drawing-ribbon-fit.json), in the Floating and the Docked
# layout: the Dimensions, Text, Symbols and Tables groups stay whole with the
# labels under the icons and without. Session 0's 1024 x 768 desktop keeps
# the window narrower than 1280 px: then the states are listed and not
# checked. The pictures are kept in -Out's pictures folder.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-ribbon-fit-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-ribbon-fit'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$pictures = Join-Path $script:UiOut 'pictures'
New-Item -ItemType Directory -Force $pictures | Out-Null
$design = Join-Path $work 'plate.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-ribbon-fit.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $pictures
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-ribbon-fit.mitcad-ui'

# Fit-States: the DRAWING tab's collapsed groups as each state left them,
# from the drawing's opening on ("opened", then "labels" and "icons" after
# each change of the labels: the last "Ribbon collapsed DRAWING: " line
# before the next change), as tools/ui-drawing-ribbon-fit-test.sh reads them.
function Fit-States {
  $states = @()
  $state = ''
  $phase = ''
  foreach ($line in Ui-LogLines) {
    if ($line.StartsWith('Ribbon collapsed DRAWING: ')) {
      $state = $line.Substring('Ribbon collapsed DRAWING: '.Length)
    } elseif ($line.StartsWith('Drawing opened: D1 ')) {
      if ($phase -eq '') { $phase = 'opened' }
    } elseif ($line -match '^Ribbon labels: (labels|icons) ') {
      if ($phase -ne '') {
        $states += , @($phase, $state)
        $phase = $Matches[1]
      }
    }
  }
  if ($phase -ne '') { $states += , @($phase, $state) }
  return , $states
}

# The shared script in one layout, then its states read from the log.
function Run-Layout([string]$Layout) {
  $env:MITCAD_TEST_LAYOUT = $Layout
  Ui-StartApp "drawing-ribbon-fit-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail "drawing ribbon fit driver timed out ($Layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "drawing ribbon fit driver failed ($Layout, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the drawing's ribbon at 1280 px ($Layout)"
  $size = Ui-LastMatch '^Window layout: window (\d+)x'
  if ($null -eq $size) { Ui-Fail "no window size logged ($Layout)" }
  $width = [int]$size
  $states = Fit-States
  if ($states.Count -eq 0) { Ui-Fail "no collapsed groups of the DRAWING tab logged ($Layout)" }
  if ($width -lt 1280) {
    foreach ($state in $states) { Write-Host "     $Layout, $($state[0]): collapsed $($state[1])" }
    Write-Host "skip the window is $width px wide, narrower than 1280 ($Layout): not checked"
    Ui-StopApp
    return
  }
  $seen = @()
  foreach ($state in $states) {
    $groups = @($state[1] -split ', ')
    foreach ($group in @('DIMENSIONS', 'TEXT', 'SYMBOLS', 'TABLES')) {
      if ($groups -contains $group) { Ui-Fail "$group collapsed at $width px ($Layout, $($state[0])): $($state[1])" }
    }
    $seen += $state[0]
    Write-Host "ok   $Layout, $($state[0]): collapsed $($state[1])"
  }
  if (-not ($seen -contains 'labels' -and $seen -contains 'icons')) {
    Ui-Fail "not both with and without labels ($Layout): $($seen -join ', ')"
  }
  Ui-StopApp
}

Run-Layout 'floating'
Run-Layout 'docked'
Remove-Item Env:MITCAD_TEST_LAYOUT -ErrorAction SilentlyContinue
Write-Host 'PASS: the drawing ribbon at 1280 px keeps Dimensions, Text, Symbols and Tables whole'
