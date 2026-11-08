# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/MainWindow.cpp app/framework/ParameterField.cpp
# check-all sources: core/sketch/src/modify.rs core/model-base/src/values.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-own-parameter.mitcad-ui tools/ui-scripts/sketch-own-parameter.json
# A sketch dimension's value that uses the dimension's own parameter keeps
# that parameter as a user parameter (mitcad#1535) in the build VM, as
# tools/ui-sketch-own-parameter-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-own-parameter.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-own-parameter.json): `Buck` in
# the width's value editor lists BuckleDiameter as kept as a user
# parameter, BuckleDiameter * 2 gives the width a new parameter and leaves
# BuckleDiameter a user parameter at 5 mm, and the height's context menu
# moves Strap to the user parameters.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-own-parameter-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-own-parameter'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'own.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-own-parameter.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-own-parameter.mitcad-ui'
Ui-StartApp 'sketch-own-parameter' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'own parameter driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "own parameter driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Sketch notice: BuckleDiameter is now a user parameter (5 mm)' 'the status bar says what moved'
Ui-ExpectLog 'Dimension k6: Strap kept as a user parameter, now Strap' 'Move Strap to User Parameters'
Ui-ExpectLog 'TestDriver: done' 'the own parameter kept as a user parameter, by an expression and by the context menu'
Ui-StopApp
Write-Host 'PASS: sketch dimension own parameter kept as a user parameter'
