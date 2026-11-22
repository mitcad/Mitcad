# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/SelectionLook.cpp app/view/SelectionLook.hpp
# check-all sources: app/view/ViewController.cpp app/MainWindow.cpp app/MainWindowSelection.cpp
# check-all sources: app/sketch/SketchOverlay.cpp app/sketch/PointMarkers.cpp app/framework/TestDriver.cpp tools/cli
# The selection's look (mitcad#1803, GitHub mitcad/Mitcad#309) in the build
# VM, as tools/ui-selection-look-test.sh on Linux: the shared steps
# tools/ui-scripts/selection-look.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/selection-look.json), at device pixel ratio 1 and
# 1.25 (QT_SCALE_FACTOR): a selected edge's highlight Selected line width's
# logical pixels times the ratio wide, wider than a plain edge, in
# Preferences > Display > Selection color; a change of either draws the
# selection anew, in the view and in sketch mode.
# Not at ratio 2, as on Linux (a 1600 x 1000 logical screen there): session
# 0's 1024 x 768 screen is 512 x 384 logical pixels at 2 and the window
# 512 x 368, narrower than the Docked window's Browser and Command panel
# alone (252 + 312 px at ratio 1, "View area 252 130 460 526"). The 3D view
# was laid out left of the window ("Command edge at -147,208"), so the top
# edge's pick place (the view's own, not clipped by the window) was outside
# it: the click reached the main window, not the view, nothing was
# selected and the run stopped at "Selected: 1 edge". At 1.25 the screen is
# 819 x 614 logical pixels, the view about 255 px wide beside the docks,
# and the ratio still not 1 (125 % is a common Windows display scale).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-selection-look-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-selection-look'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
foreach ($scale in @(1, 1.25)) {
  # Settings of the run's own: each starts with the defaults.
  Ui-Init $App $Out $QtBin @{}
  $work = Join-Path $script:UiOut 'files'
  New-Item -ItemType Directory -Force $work | Out-Null
  $design = Join-Path $work 'selection-look.mitcad'
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/selection-look.json') --save $design
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
  $env:MITCAD_TEST_WORK = $work
  $env:MITCAD_TEST_STEP_MS = '50'
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/selection-look.mitcad-ui'
  $env:MITCAD_TEST_PREFERENCES_DONE = 'Return' # OK of the Preferences dialog
  if ($scale -eq 1) {
    Remove-Item Env:QT_SCALE_FACTOR -ErrorAction SilentlyContinue
  } else {
    $env:QT_SCALE_FACTOR = "$scale"
  }
  Remove-Item Env:MITCAD_USER_QT_SCALE_FACTOR -ErrorAction SilentlyContinue
  Write-Host "=== Device pixel ratio $scale"
  Ui-StartApp "selection-look-$scale" @('--ui=qt', '--no-mcp', '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'selection look driver timed out' }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "selection look driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'View selection look: color #1f7aff, line width normal; ' 'Normal and the selection blue by default'
  Ui-ExpectLog 'Preferences: selection color #d02090, line width normal' 'Selection color saved'
  Ui-ExpectLog 'Preferences: selection color #d02090, line width thick' 'Selected line width saved'
  Ui-ExpectLog 'TestDriver: done' 'the selection drawn in the chosen color and width, in the view and in sketch mode'
  # Each highlight line drawn at its logical width times this run's ratio,
  # wider than a plain edge; the steps' widths and colors in their order.
  # Only the lines that begin with it, as on Linux: the driver's echo of
  # each step ("TestDriver: line 30: wait-log '^View highlight line: ...")
  # holds the text too.
  $pattern = '^View highlight line: ([a-z ]+) ([0-9.]+) px \(([0-9.]+) x ratio ([0-9.]+)\), color (#[0-9a-f]{6}); plain edges ([0-9.]+) px$'
  $seen = @()
  foreach ($line in @(Ui-LogLines | Where-Object { $_.StartsWith('View highlight line: ') })) {
    if ($line -notmatch $pattern) { Ui-Fail "unexpected highlight line: $line" }
    $drawn = [double]$Matches[2]
    $logical = [double]$Matches[3]
    $ratio = [double]$Matches[4]
    $plain = [double]$Matches[6]
    if ([math]::Abs($drawn - $logical * $ratio) -gt 1e-3 -or [math]::Abs($ratio - $scale) -gt 1e-3 -or
        $drawn -le $plain) {
      Ui-Fail "$($Matches[1]) drawn $drawn px: not $logical x the ratio $scale, or not wider than a plain edge ($plain px)"
    }
    Write-Host "ok   $($Matches[1]) highlight $drawn px ($logical x ratio $ratio) in $($Matches[5])"
    $seen += "$($Matches[1]) $drawn $($Matches[5])"
  }
  $expected = @("edge $(4 * $scale) #1f7aff", "edge $(4 * $scale) #d02090", "edge $(6 * $scale) #d02090",
                "sketch curve $(6 * $scale) #d02090")
  $next = 0
  foreach ($entry in $seen) {
    if ($next -lt $expected.Count -and $entry -eq $expected[$next]) { $next++ }
  }
  if ($next -lt $expected.Count) {
    Ui-Fail "no highlight line '$($expected[$next])' in its turn among: $($seen -join '; ')"
  }
  Ui-StopApp
}
Remove-Item Env:QT_SCALE_FACTOR -ErrorAction SilentlyContinue
Write-Host 'PASS: the selection is drawn in Selection color, Selected line width times the device pixel ratio wide'
