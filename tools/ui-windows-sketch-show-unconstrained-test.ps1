# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/sketch app/commands/SketchCommands.cpp app/framework/TestDriver.cpp
# check-all sources: core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-show-unconstrained.mitcad-ui
# check-all sources: tools/ui-scripts/sketch-show-unconstrained.json
# Show Unconstrained (mitcad#2026) on Windows, the shared input script
# tools/ui-scripts/sketch-show-unconstrained.mitcad-ui as
# tools/ui-sketch-show-unconstrained-test.sh runs it on Linux: a sketch with
# one free line and two fully constrained ones; the Sketch Palette's Show
# Unconstrained button, a click on the degrees of freedom badge and the
# sketch's context menu each select exactly the free line and its two
# points.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-show-unconstrained-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-show-unconstrained'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'unconstrained.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-show-unconstrained.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-show-unconstrained.mitcad-ui'
Ui-StartApp 'sketch-show-unconstrained' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'sketch show unconstrained driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch show unconstrained driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Palette: Show Unconstrained' 'the palette button ran Show Unconstrained'
Ui-ExpectLog 'Sketch badge clicked' 'a click on the degrees of freedom badge ran it'
Ui-ExpectLog 'Show Unconstrained: 1 curve(s), 2 point(s): ' 'the free line and its two points selected'
Ui-ExpectLog 'TestDriver: done' 'Show Unconstrained from the palette, the badge and the context menu'
Ui-StopApp
Write-Host 'PASS: sketch show unconstrained'
