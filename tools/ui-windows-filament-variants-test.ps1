# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/framework/TestDriver.cpp core/model/src/material.rs core/model/src/print_files app/browser/LibraryPicker.cpp app/framework/CommandPanel.cpp
# One filament, its variants per printer and nozzle (mitcad#1054), in the
# build VM: the same driver script as the Linux and macOS tests
# (tools/ui-scripts/filament-variants.mitcad-ui: 3D Print's Materials row,
# Import Presets grouped by filament with the printers in use and the
# merge of separate materials, the Materials window's Variants tab) on a
# design mitcad-cli makes, with the synthetic slicer folder
# tools/ui-scripts/filament-variants-slicer as the installed slicer's.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-filament-variants'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'variants.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/filament-variants.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'filament variants fixture creation failed' }
# The user's library in the test's own folder: only the design's printers
# are offered.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $script:UiOut 'print-library'
$slicer = Join-Path $PSScriptRoot 'ui-scripts/filament-variants-slicer'
$env:MITCAD_TEST_SLICER_FOLDERS = ConvertTo-Json -Compress -InputObject @(@{ dialect = 'bambu_studio'; path = $slicer })
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/filament-variants.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'variants' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'filament variants driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "filament variants driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'variants in 3D Print, Import Presets and the Materials window passed'
Ui-StopApp
Write-Host 'PASS: filament variants'
