# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser/BrowserController.cpp app/MainWindowSheet.cpp
# check-all sources: app/framework/CommandRegistry.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/drawing-flat-pattern-commands.mitcad-ui tools/ui-scripts/drawing-flat-pattern-commands.json
# The Flat Pattern's drawing commands (mitcad#2446) in the build VM, as
# tools/ui-drawing-flat-pattern-commands-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-flat-pattern-commands.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/drawing-flat-pattern-commands.json):
# Base View's Shows has the Flat Pattern after a separator; Flat Pattern View
# from the DRAWING tab's Create menu places the blank with its bend line;
# Create Drawing of Flat Pattern... in the Flat Pattern row's menu makes a
# drawing with the blank placed on it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-flat-pattern-commands-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-flat-pattern-commands'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plate.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-flat-pattern-commands.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-flat-pattern-commands.mitcad-ui'
Ui-StartApp 'drawing-flat-pattern-commands' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'flat pattern commands driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "flat pattern commands driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Drawing tool: Flat Pattern View' 'Flat Pattern View from the Create menu'
Ui-ExpectLog 'Drawing Flat Pattern placed: view 1 of F3.b0' 'Create Drawing of Flat Pattern from the Flat Pattern row'
Ui-ExpectLog 'Drawing Flat Pattern views: view 1: Plate, 50.65 x 20.00, 1 bend line' 'the blank drawn with its bend line'
Ui-ExpectLog 'TestDriver: done' 'both commands placed the Flat Pattern'
Ui-StopApp
Write-Host 'PASS: drawing Flat Pattern commands'
