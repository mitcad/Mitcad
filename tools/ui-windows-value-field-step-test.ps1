# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ValueStep.cpp app/framework/TestDriver.cpp tools/cli
# Up, Down and the mouse wheel step a value field's number (mitcad#1254,
# GitHub mitcad/Mitcad#129) in the build VM, as
# tools/ui-value-field-step-test.sh on Linux: the shared steps
# tools/ui-scripts/value-field-step.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/value-field-step.json): Extrude's Distance
# holding d1*2 is not stepped, 10 mm and Shift+Up is 20 mm; Circular
# Pattern's Quantity 4 and Up is 5, previewed; the wheel over a field
# without the keyboard changes nothing, over the one with it steps it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-value-field-step-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-value-field-step'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'value-field-step.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/value-field-step.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/value-field-step.mitcad-ui'
Ui-StartApp 'value-field-step' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'value field step driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "value field step driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Value field Extrude Distance: Up not stepped: d1*2 is not a plain value' 'an expression is not stepped'
Ui-ExpectLog 'Value field Extrude Distance: Shift+Up stepped 10 mm to 20 mm' 'Shift+Up steps a distance by 10 mm'
Ui-ExpectLog 'Value field Circular Pattern Quantity: Up stepped 4 to 5' 'Up steps the quantity from 4 to 5'
Ui-ExpectLog 'Value field Circular Pattern Quantity: wheel stepped 5 to 6' 'the wheel steps the field with the keyboard'
Ui-ExpectLog 'TestDriver: done' 'Up, Down and the wheel step the values'
Ui-StopApp
Write-Host 'PASS: value fields step with Up, Down and the wheel'
