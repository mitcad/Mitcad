# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterField.cpp app/framework/ValueStep.cpp app/framework/TestDriver.cpp tools/cli
# Up, Down and the mouse wheel step a value field's number (mitcad#1254,
# GitHub mitcad/Mitcad#129), and so do the step buttons at its right end
# and the 3D view's value boxes (mitcad#2327, GitHub mitcad/Mitcad#594),
# in the build VM, as tools/ui-value-field-step-test.sh on Linux: the
# shared steps tools/ui-scripts/value-field-step.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/value-field-step.json): Extrude's
# Distance holding d1*2 is not stepped, by Up nor by its disabled up
# button, 10 mm and Shift+Up is 20 mm, the up button 21 mm, Shift and the
# up button 31 mm, the down button held steps it down again and again;
# Box's Height in its value box in the view steps with Up, Shift+Up and
# the wheel, the view neither turned nor zoomed; Circular Pattern's
# Quantity 4 and Up is 5, previewed; the wheel over a field without the
# keyboard changes nothing, over the one with it steps it; a sketch
# rectangle's Width 40 mm and its up button is 41 mm.
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
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'value field step driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "value field step driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Value field Extrude Distance: Up not stepped: d1*2 is not a plain value' 'an expression is not stepped'
Ui-ExpectLog 'Value field Extrude Distance: Shift+Up stepped 10 mm to 20 mm' 'Shift+Up steps a distance by 10 mm'
Ui-ExpectLog 'Value field Circular Pattern Quantity: Up stepped 4 to 5' 'Up steps the quantity from 4 to 5'
Ui-ExpectLog 'Value field Circular Pattern Quantity: wheel stepped 5 to 6' 'the wheel steps the field with the keyboard'
Ui-ExpectLog 'Value field Extrude Distance: Up button stepped 20 mm to 21 mm' 'the up button steps a distance by 1 mm'
Ui-ExpectLog 'Value field Extrude Distance: Shift+Up button stepped 21 mm to 31 mm' 'Shift and the up button step by 10 mm'
Ui-ExpectLog 'Value field Extrude Distance: Down button held stepped 30 mm to 29 mm' 'the down button held repeats'
Ui-ExpectLog 'Value field Box Height: Shift+Up stepped 11 mm to 21 mm' "Shift+Up steps the view's value box"
Ui-ExpectLog 'Value field Box Height: wheel stepped 21 mm to 22 mm' "the wheel steps the view's value box"
Ui-ExpectLog 'Value field sketch width: Up button stepped 40 mm to 41 mm' "a sketch tool field's up button steps it"
Ui-ExpectLog 'TestDriver: done' 'Up, Down, the wheel and the step buttons step the values'
Ui-StopApp
Write-Host 'PASS: value fields step with Up, Down, the wheel and the step buttons'
