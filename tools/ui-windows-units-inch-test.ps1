# SPDX-License-Identifier: MIT
# check-all sources: app/framework/LengthUnits.cpp app/commands/InspectCommands.cpp tools/cli
# Lengths in the design's unit (mitcad#1165) in the build VM, as
# tools/ui-units-inch-test.sh on Linux: the shared steps
# tools/ui-scripts/units-inch.mitcad-ui on the design in inches that
# mitcad-cli makes (tools/cli/tests/units_inch.json): the Browser's
# sheet-rules row in inches, Extrude's default distance as 0.5 in, Measure's
# area and distances in inches.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-units-inch-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-units-inch'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'inch.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'cli/tests/units_inch.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design in inches was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/units-inch.mitcad-ui'
Ui-StartApp 'units-inch' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'units in inches driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "units in inches driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the sheet-rules row, Extrude and Measure in inches'
Ui-StopApp
Write-Host 'PASS: lengths in the design unit'
