# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParameterSort.cpp app/framework/ParameterUnits.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-sort.mitcad-ui tools/ui-scripts/parameters-sort.json
# Edit Parameters sorted by a column (mitcad#1866, GitHub mitcad/Mitcad#338)
# in the build VM, as tools/ui-parameters-sort-test.sh on Linux: the shared
# steps tools/ui-scripts/parameters-sort.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/parameters-sort.json): Name's and
# Value's headers sort the rows within their groups, both ways and back to
# the creation order; the choice is kept; an edited and an added row stay
# selected in their place.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-parameters-sort-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameters-sort'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'parameters-sort.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameters-sort.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameters-sort.mitcad-ui'
Ui-StartApp 'parameters-sort' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'the parameters sort driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the parameters sort driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Parameters sorted by name ascending' 'Name''s header sorts the rows'
Ui-ExpectLog 'Parameters sorted by value descending' 'Value''s header twice sorts them in the reverse order'
Ui-ExpectLog 'Parameters sorted in creation order' 'a third click goes back to the creation order'
Ui-ExpectLog 'TestDriver: done' 'the rows sort within their groups by a column'
Ui-StopApp
Write-Host 'PASS: Edit Parameters sorted by a column'
