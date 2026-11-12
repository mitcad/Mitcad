# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindow.hpp app/OcctViewer.cpp app/OcctViewer.hpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/sketch/SketchPalette.cpp
# check-all sources: app/sketch/SketchPreferences.cpp app/sketch/SketchPreferences.hpp app/view/ViewController.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-profiles-on-open.mitcad-ui tools/ui-scripts/sketch-profiles-on-open.json
# Preferences > Sketch > Show profiles when a sketch opens (mitcad#1670)
# in the build VM, as tools/ui-sketch-profiles-on-open-test.sh on Linux:
# the shared steps tools/ui-scripts/sketch-profiles-on-open.mitcad-ui on the
# design that mitcad-cli makes (tools/ui-scripts/sketch-profiles-on-open.json):
# on (the default), a sketch opens with Show profiles checked and its region
# shaded; turned off in Preferences, each sketch opens with it unchecked and
# no region shaded, the palette's checkbox shades it for that sketch only,
# and the next sketch opens with it off again; turned on again, sketches
# open as before.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-profiles-on-open-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-profiles-on-open'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'profiles.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-profiles-on-open.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-profiles-on-open.mitcad-ui'
Ui-StartApp 'sketch-profiles-on-open' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sketch profiles on open driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch profiles on open driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch Sketch1 opens with Show profiles on' 'on (the default): Sketch1 opens with its region shaded'
Ui-ExpectLog 'Preferences: show profiles when a sketch opens off' 'the preference turned off in Preferences > Sketch'
Ui-ExpectLog 'Sketch Sketch1 opens with Show profiles off' 'off: Sketch1 opens with no region shaded'
Ui-ExpectLog 'Show profiles on' 'the palette shades the region for that sketch'
Ui-ExpectLog 'Sketch Sketch2 opens with Show profiles off' 'off: the next sketch opens with no region shaded again'
Ui-ExpectLog 'Preferences: show profiles when a sketch opens on' 'the preference turned on again'
Ui-ExpectLog 'Sketch Sketch2 opens with Show profiles on' 'on again: sketches open as before'
Ui-ExpectLog 'TestDriver: done' 'Show profiles when a sketch opens'
Ui-StopApp
Write-Host 'PASS: sketch profiles on open'
