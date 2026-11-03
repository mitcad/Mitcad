# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/MainWindowMake.cpp app/framework/TestDriver.cpp core/model/src/print_profile.rs core/model/src/document/library_items.rs
# Processes grouped by printer (mitcad#1216), in the build VM: the same
# driver script as the Linux test
# (tools/ui-scripts/processes-by-printer.mitcad-ui: the Processes
# manager's sub-groups by printer, Send to Slicer's Process picker with the
# chosen printer's processes first, the Make tab's body card) on a design
# mitcad-cli makes from tools/ui-scripts/processes-by-printer.json.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-processes-by-printer'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'processes.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/processes-by-printer.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'processes by printer fixture creation failed' }
# The user's library in the test's own folder: empty.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $script:UiOut 'print-library'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/processes-by-printer.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'processes' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'processes by printer driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "processes by printer driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'processes by printer in the Processes manager, the Process picker and the body card passed'
Ui-StopApp
Write-Host 'PASS: processes by printer'
