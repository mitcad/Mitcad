# SPDX-License-Identifier: MIT
# check-all sources: app/print/MaterialProperties.cpp app/print/MaterialsDialog.cpp app/print/PrintLibrary.cpp app/framework/TestDriver.cpp core/model/src/material_dataset.rs core/model/data/wood_materials.json
# check-all sources: core/model-base/src/material_class.rs core/model-base/src/appearance.rs core/model-api/src/materials.rs
# The bundled dataset's woods and panels in the class Wood (mitcad#1552),
# in the build VM: the same driver script as the Linux test
# (tools/ui-scripts/wood-materials.mitcad-ui: the Materials window's New
# menu with Wood, Browse Material Dataset... with its two sources, "oak"
# found in the class Wood with its values and source, and Wood - Oak
# created into the design in the Materials window's Wood group with its
# look) on a design mitcad-cli makes. The pictures go to the output folder.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-wood-materials'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$design = Join-Path $script:UiOut 'wood.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/wood-materials.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'wood materials fixture creation failed' }
# The user's library in the test's own folder: no materials of the user's.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $script:UiOut 'print-library'
$env:MITCAD_TEST_WORK = $script:UiOut
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/wood-materials.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'wood' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'wood materials driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "wood materials driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'oak found in the class Wood and created as a Wood material of the design'
Ui-StopApp
Write-Host 'PASS: wood materials'
