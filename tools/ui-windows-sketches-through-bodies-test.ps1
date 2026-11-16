# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp app/view/ViewController.hpp
# check-all sources: app/browser/BrowserController.cpp app/MainWindowVisibility.cpp app/PickContext.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# Sketches through the bodies outside sketch mode (mitcad#1931) in the
# build VM, as tools/ui-sketches-through-bodies-test.sh on Linux: the shared
# steps tools/ui-scripts/sketches-through-bodies.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/sketches-through-bodies.json): the
# profile a block hides drawn through it, picked in Extrude and joined to
# the block; Show All Sketches, Sketches Through Bodies off and on, Hide
# All Sketches and its undo.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketches-through-bodies-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketches-through-bodies'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'through.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketches-through-bodies.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketches-through-bodies.mitcad-ui'
Ui-StartApp 'sketches-through-bodies' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sketches through bodies driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketches through bodies driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketches through the bodies: F2 4 curve(s)' 'the finished sketch and its profile drawn through the block'
Ui-ExpectLog 'Added extrude (Join)' 'the hidden profile picked and joined to the block'
Ui-ExpectLog 'All sketches Root: shown (1)' 'Show All Sketches shows the used sketch'
Ui-ExpectLog 'Sketches through the bodies: off' 'Sketches Through Bodies off: hidden again'
Ui-ExpectLog 'All sketches Root: hidden (1)' 'Hide All Sketches hides it'
Ui-ExpectLog 'TestDriver: done' 'sketches through the bodies outside sketch mode'
Ui-StopApp
Write-Host 'PASS: sketches through bodies'
