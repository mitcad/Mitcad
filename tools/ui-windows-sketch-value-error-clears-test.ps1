# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/MessageBand.cpp app/framework/MessageLog.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/sketch-value-error-clears.mitcad-ui
# check-all sources: tools/ui-scripts/sketch-value-error-clears.json
# A sketch value editor's error goes when the editor closes (mitcad#1621)
# in the build VM, as tools/ui-sketch-value-error-clears-test.sh on Linux:
# the shared steps tools/ui-scripts/sketch-value-error-clears.mitcad-ui on
# the design that mitcad-cli makes
# (tools/ui-scripts/sketch-value-error-clears.json), in the Docked and the
# Floating layout: an empty value says "Enter a value." without a leading
# colon, Esc takes the error from the status line and the message band (the
# history keeps it), and a value taken does too.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-value-error-clears-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-value-error-clears'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'value.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-value-error-clears.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-value-error-clears.mitcad-ui'
foreach ($chrome in @('docked', 'floating')) {
  Write-Host "--- $chrome"
  Ui-StartApp "sketch-value-error-clears-$chrome" @('--ui=qt', '--no-mcp', "--chrome=$chrome", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${chrome}: value error driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${chrome}: value error driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'Status error: Enter a value.' "${chrome}: the empty value's error, without a leading colon"
  Ui-ExpectLog 'Status line cleared (sketch/editor closed)' "${chrome}: Esc took it from the status line"
  Ui-ExpectLog 'Message band source closed: error Enter a value. (sketch/editor)' "${chrome}: and from the message band"
  Ui-ExpectLog 'Dimension k5 = 25 mm (d1)' "${chrome}: the value taken"
  Ui-ExpectLog 'TestDriver: done' "${chrome}: the error goes when the editor closes or takes its value"
  Ui-StopApp
}
Write-Host 'PASS: sketch value error clears'
