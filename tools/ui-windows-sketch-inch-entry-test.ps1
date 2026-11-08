# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/DrawTools.cpp app/browser/ParametersDialog.cpp
# check-all sources: core/model-base/src/values.rs tools/cli
# check-all sources: core/expr/src/lexer.rs core/expr/src/parser.rs
# check-all sources: tools/ui-scripts/sketch-inch-default-units.mitcad-ui app/MainWindowStart.cpp
# Lengths typed in a design in inches are stored as inches (mitcad#1452)
# in the build VM, as tools/ui-sketch-inch-entry-test.sh on Linux: the
# shared steps tools/ui-scripts/sketch-inch-entry.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/sketch-inch-entry.json): Add
# User Parameter in inches, a dimension made in mm set to 0.25 * 2 as
# 12.7 mm, a rectangle typed 0.875 by 12.125 and a circle typed 0.2 stored
# as 22.225 x 307.975 mm and 5.08 mm. Then the reported sequence on a
# fresh start with the Start page shown
# (tools/ui-scripts/sketch-inch-default-units.mitcad-ui): Default units for
# new designs set to in, the empty untitled design starts again in inches
# and a rectangle typed .875 by 12.25 is 22.225 x 311.15 mm.
# The shared steps also set the design to mm and type a circle with the
# inch mark, .875", stored as 22.225 mm (mitcad#1531).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-inch-entry-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-inch-entry'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'inch.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-inch-entry.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design in inches was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-inch-entry.mitcad-ui'
Ui-StartApp 'sketch-inch-entry' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'sketch values in inches driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch values in inches driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Add User Parameter dialog opened: unit in' 'a new parameter starts in inches'
Ui-ExpectLog 'TestDriver: done' 'typed values, a dimension made in mm and a new parameter, all stored as inches'
Ui-StopApp

# The reported sequence (tools/ui-scripts/sketch-inch-default-units.mitcad-ui):
# a fresh start on the Start page, the default units set to in, the
# untitled design drawn in by typing.
$env:MITCAD_START_PAGE = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-inch-default-units.mitcad-ui'
Ui-StartApp 'sketch-inch-default-units' @('--ui=qt', '--no-mcp')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'default units driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "default units driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'The untitled design starts again in in' 'the empty untitled design takes the new default units'
Ui-ExpectLog 'Start page new design units: in' "the Start page's New Design card says in"
Ui-ExpectLog 'TestDriver: done' 'the default units set after the start, then drawn by typing, stored as inches'
Ui-StopApp
Write-Host 'PASS: sketch values typed in a design in inches'
