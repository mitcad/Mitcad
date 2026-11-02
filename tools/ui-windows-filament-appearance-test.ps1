# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserMaterialMenu.cpp app/framework/TestDriver.cpp core/model/src/material.rs core/model/src/document/materials.rs app/browser/AppearanceDialog.cpp app/commands/MaterialCommands.cpp
# check-all sources: core/model-base/src/material_class.rs core/model-base/src/appearance.rs
# A filament's colour is its appearance (mitcad#1055), in the build VM: the
# same driver script as the Linux and macOS tests
# (tools/ui-scripts/filament-appearance.mitcad-ui: Material > PLA Basic >
# an appearance for bodies and the root, a long material name shown in
# full in the Materials window's list, General's list of a material's
# appearances with Add... (the appearance library's popover, the material's
# class first, mitcad#1102), a filament's Class fixed, Make Default
# and Remove, the Appearances window grouped by class with the filament
# colours under FDM Plastic and the Appearance tool's picker on a PLA body,
# mitcad#1133) on a design mitcad-cli makes. The pictures of the Materials
# window and the picker go to the output folder.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-filament-appearance'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'appearance.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/filament-appearance.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'filament appearance fixture creation failed' }
# The user's library in the test's own folder: no recent materials.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $script:UiOut 'print-library'
$env:MITCAD_TEST_WORK = $script:UiOut
$env:MITCAD_TEST_COLOR_SCHEME = 'light'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/filament-appearance.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'appearance' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'filament appearance driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "filament appearance driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the appearances of the Material submenu and the Materials window passed'
Ui-StopApp
Write-Host 'PASS: filament appearance'
