# SPDX-License-Identifier: MIT
# check-all sources: app/browser/Configuration app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli core/model-api/src/configurations.rs
# Configurations (mitcad#807) in the build VM, through the test driver
# (tools/ui-scripts/configurations.mitcad-ui, the same script as the macOS
# test's): the Configurations window opens with its options and row, a
# choice in the browser's Configuration bar changes the body, and a
# parameter column added in the Rows tab (mitcad#999) drives the body when
# its row is applied.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-configurations'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'configurations.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/configurations.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'configurations fixture creation failed' }
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/configurations.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'configurations' @('--ui=qt', '--no-mcp', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'configurations driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "configurations driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the window opened, the bar''s choice changed the body and a parameter column drove it'
Ui-ExpectLog 'Configurations: column Extra added' 'Add Column made Extra a column'
Ui-ExpectLog 'Configurations: column Extra removed' 'Remove Column took it out'
Ui-StopApp
Write-Host 'PASS: configurations'
