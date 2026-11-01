# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationsDialog.cpp app/browser/ConfigurationsData.cpp app/browser/ConfigurationBar.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli core/model-api/src/configuration_rows_csv.rs
# Filling a closed table (mitcad#1044) in the build VM, through the test
# driver (tools/ui-scripts/configuration-rows.mitcad-ui), as
# tools/ui-configuration-rows-test.sh does on Linux: the Rows tab's Import
# CSV... reads a small table of screw sizes through its preview into a
# closed table in one undo step, Add Option... moves a parameter column to
# the options, and a size chosen on the Configuration bar sets the row's
# parameter column k: the body follows.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-configuration-rows'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'configuration-rows.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/configuration-rows.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'configuration rows fixture creation failed' }
$csv = Join-Path $script:UiOut 'screws.csv'
[IO.File]::WriteAllText($csv, "size,d [mm],k [mm]`r`nM3,3,3`r`nM4,4,4`r`nM5,5,5`r`n")
$env:MITCAD_ROWS_CSV = $csv
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/configuration-rows.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'configuration-rows' @('--ui=qt', '--no-mcp', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'configuration rows driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "configuration rows driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the rows imported from a CSV, an option added to the closed table and a size chosen on the bar'
Ui-ExpectLog 'Configurations imported rows from screws.csv' 'Import CSV filled the rows'
Ui-ExpectLog 'Configurations option d moved from the parameter columns' 'Add Option took a parameter column'
Ui-StopApp
Write-Host 'PASS: configuration rows'
