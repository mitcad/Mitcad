# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SelectionScope.cpp app/framework/SelectionScope.hpp app/framework/SelectionFilterPanel.cpp
# check-all sources: app/MainWindowSelection.cpp app/sketch app/framework/TestDriver.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-filter-annotations.mitcad-ui tools/ui-scripts/sketch-filter-annotations.json
# The sketch Selection Filter's Dimensions and Constraints rows (GitHub
# #388) in the build VM, as tools/ui-sketch-filter-annotations-test.sh on
# Linux: the shared steps tools/ui-scripts/sketch-filter-annotations.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/sketch-filter-annotations.json). With only Dimensions
# a cross a window takes only the dimension; with only Constraints a cross
# only the constraints, the Fix symbol among them; with Dimensions empty a
# click on the dimension does not select it, one on the Parallel does.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-filter-annotations-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-filter-annotations'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'annotations.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-filter-annotations.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-filter-annotations.mitcad-ui'
Ui-StartApp 'sketch-filter-annotations' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch filter annotations driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch filter annotations driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Selection filter sketch dimensions cross: ' 'only Dimensions a cross'
Ui-ExpectLog 'Selection filter sketch constraints cross: ' 'only Constraints a cross'
Ui-ExpectLog 'Selection filter sketch dimensions empty: ' 'Dimensions empty'
Ui-ExpectLog 'TestDriver: done' 'a window and a click each followed their own row'
Ui-StopApp
Write-Host 'PASS: sketch filter Dimensions and Constraints rows'
