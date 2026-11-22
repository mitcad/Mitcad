# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/framework/ParameterUnits.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-new-row.mitcad-ui tools/ui-scripts/parameters-new-row.json
# The Parameters window's empty row, its rows' delete buttons and its
# bottom row (mitcad#2202, GitHub mitcad/Mitcad#512) in the build VM, as
# tools/ui-parameters-new-row-test.sh on Linux: the shared steps
# tools/ui-scripts/parameters-new-row.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/parameters-new-row.json): parameters
# typed one after the other in the empty row, a taken name refused, an
# unused parameter deleted by its row's button and a used one's button
# disabled, the empty row last under a sort and hidden under the search, a
# slider's value shown during the drag, and the bottom row's pictures on
# both tabs (in the output folder).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-parameters-new-row-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameters-new-row'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'parameters-new-row.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameters-new-row.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_PARAMETERS_SHOTS = $script:UiOut
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameters-new-row.mitcad-ui'
Ui-StartApp 'parameters-new-row' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'the parameters new row driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the parameters new row driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Created user parameter a3 = 13 mm (mm) in the new row' 'three parameters typed one after the other'
Ui-ExpectLog 'New user parameter spare refused: ' 'a taken name is refused and kept'
Ui-ExpectLog 'Deleted user parameter spare by its row button' 'an unused parameter deleted by its row''s button'
Ui-ExpectLog 'Parameter slide slider held at ' 'the slider''s value shown before the release'
Ui-ExpectLog 'TestDriver: done' 'the empty row, the delete buttons and the bottom row'
foreach ($tab in @('parameters', 'tables')) {
  $shot = Join-Path $script:UiOut "parameters-bottom-row-$tab.png"
  if (-not (Test-Path $shot)) { Ui-Fail "no picture of the bottom row on the $tab tab" }
}
Ui-StopApp
Write-Host 'PASS: Parameters window: the empty row, the delete buttons and the bottom row'
