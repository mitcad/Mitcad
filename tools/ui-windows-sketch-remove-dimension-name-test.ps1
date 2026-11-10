# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchController.cpp app/sketch/SketchController.hpp app/MainWindow.cpp
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document.rs core/model-api/src/lib.rs core/expr/src/table.rs
# check-all sources: core/sketch/src/modify.rs
# check-all sources: tools/ui-scripts/sketch-remove-dimension-name.mitcad-ui
# check-all sources: tools/ui-scripts/sketch-remove-dimension-name.json
# A name given to a sketch dimension's parameter taken off again
# (mitcad#1577) in the build VM, as tools/ui-sketch-remove-dimension-name-test.sh
# on Linux: the shared steps tools/ui-scripts/sketch-remove-dimension-name.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/sketch-remove-dimension-name.json): the name deleted in
# the value editor, Remove Name in its right-click menu and in the
# dimension's context menu give the automatic name back, Backspace twice
# takes the whole name off and what is typed then goes in with the
# automatic name (mitcad#1614), and the text
# replaced by the bare name keeps the parameter as a user parameter
# without a refusal (mitcad#1608).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-remove-dimension-name-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-remove-dimension-name'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'names.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-remove-dimension-name.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-remove-dimension-name.mitcad-ui'
Ui-StartApp 'sketch-remove-dimension-name' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'remove dimension name driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "remove dimension name driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Dimension k5: name width removed, now d1 (rewritten: d2)' 'the name taken off, k6 rewritten'
Ui-ExpectLog 'Undo: Remove Name width' 'one undo step'
Ui-ExpectLog 'Parameter name width removed for dimension width: 20 mm' "Remove Name in the value editor's menu"
Ui-ExpectLog 'Parameter name width removed by Backspace for dimension width' 'Backspace on the empty value takes the name off'
Ui-ExpectLog 'Dimension k5 = spacing (d1)' 'the automatic name follows another parameter typed'
Ui-ExpectLog 'Dimension k5: width kept as a user parameter, now width' 'the bare name keeps width as a user parameter'
Ui-ExpectLog 'TestDriver: done' 'the name removed in the editor, its menu and the context menu'
Ui-StopApp
Write-Host 'PASS: sketch dimension name removed'
