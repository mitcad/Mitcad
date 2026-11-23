# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/framework/ParameterField.cpp app/framework/FormulaEditor.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-fx.mitcad-ui tools/ui-scripts/parameters-new-row.json
# Edit Parameters' Expression cells' fx (mitcad#2290) in the build VM, as
# tools/ui-parameters-fx-test.sh on Linux: the shared steps
# tools/ui-scripts/parameters-fx.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/parameters-new-row.json): entering an Expression
# cell opens no function list; the cell's fx opens Edit Formula with the
# text typed so far on an existing row and on the empty row, whose OK
# makes the parameter; the bottom Edit Formula... uses an edited cell's
# text and works on the empty row.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-parameters-fx-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameters-fx'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'parameters-fx.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameters-new-row.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameters-fx.mitcad-ui'
Ui-StartApp 'parameters-fx' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'the parameters fx driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the parameters fx driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Formula editor opened for parameter spare with: 7' 'an existing row''s fx opens Edit Formula'
Ui-ExpectLog 'Formula editor opened for new parameter b1 with: 8' 'the empty row''s fx opens Edit Formula'
Ui-ExpectLog 'Created user parameter b1 = 9 mm (mm) in the new row' 'the empty row''s parameter made from Edit Formula'
Ui-ExpectLog 'Formula editor opened for parameter used with: 21' 'Edit Formula with the text typed in the cell'
Ui-ExpectLog 'Formula editor for new parameter (no name) canceled' 'Edit Formula on the empty row'
Ui-ExpectLog 'TestDriver: done' 'the Expression cells'' fx'
Ui-StopApp
Write-Host 'PASS: Parameters window: the Expression cells'' fx opens Edit Formula'
