# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ParameterField.hpp
# check-all sources: app/framework/ParameterSuggest.cpp app/framework/CommandPanel.cpp
# check-all sources: app/sketch/SketchController.cpp app/sketch/DrawTools.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/parameter-unknown-name.mitcad-ui
# check-all sources: tools/ui-scripts/parameter-unknown-name.json
# An unknown name and Enter make a user parameter in every value field
# (mitcad#1723) in the build VM, as tools/ui-parameter-unknown-name-test.sh
# on Linux: the shared steps tools/ui-scripts/parameter-unknown-name.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/parameter-unknown-name.json): the Rectangle tool's
# Width, a new sketch dimension and Extrude1's Distance each open Add User
# Parameter, and name = value typed out still names the dimension's own
# parameter.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-parameter-unknown-name-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameter-unknown-name'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'unknown.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameter-unknown-name.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameter-unknown-name.mitcad-ui'
Ui-StartApp 'parameter-unknown-name' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'unknown name driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "unknown name driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Add User Parameter opened for sketch width: name rect_side' "a drawing tool's field: Add User Parameter"
Ui-ExpectLog 'Created user parameter rect_side = 10 mm (mm) for sketch width' 'rect_side made'
Ui-ExpectLog 'Add User Parameter opened for sketch dimension: name line_len' 'a sketch dimension: Add User Parameter'
Ui-ExpectLog 'Created user parameter line_len = 25 mm (mm) for sketch dimension' 'line_len made'
Ui-ExpectLog 'Dimension k5 = side = 18 mm (side)' 'name = value still names the dimension'
Ui-ExpectLog 'Add User Parameter opened for Extrude Distance: name ext_height' "a panel's field: Add User Parameter"
Ui-ExpectLog 'Created user parameter ext_height = 12 mm (mm) for Extrude Distance' 'ext_height made'
Ui-ExpectLog 'TestDriver: done' 'an unknown name and Enter make a user parameter in every field'
Ui-StopApp
Write-Host 'PASS: parameter unknown name'
