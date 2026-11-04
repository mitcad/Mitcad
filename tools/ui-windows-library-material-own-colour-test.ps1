# SPDX-License-Identifier: MIT
# check-all sources: app/print/MaterialsDialog.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/library_items.rs core/model/src/print_files/mod.rs
# check-all sources: core/model-api/src/materials.rs core/model/src/print_library.rs
# A My library material whose file lacks its own color (mitcad#1242), in
# the build VM: the same driver script as the Linux test
# (tools/ui-scripts/library-material-own-colour.mitcad-ui: the Materials
# window renames the item, the new name listed) on My library made from
# tools/ui-scripts/library-material-own-colour/, no design open. Then the
# item's file: the new name and its own color made again and carried.
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-library-material-own-colour'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
# My library in the test's own folder: the item without its own color.
$library = Join-Path $script:UiOut 'print-library'
Remove-Item -Recurse -Force $library -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $library | Out-Null
Copy-Item -Recurse -Force (Join-Path $PSScriptRoot 'ui-scripts/library-material-own-colour/*') $library
$item = Join-Path $library 'materials/generic-tpu-x.json'
if (-not (Test-Path $item)) { Ui-Fail "the fixture is not in $library" }
$env:MITCAD_PRINT_LIBRARY_DIR = $library
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/library-material-own-colour.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '70'
Ui-StartApp 'library-material-own-colour' @('--ui=qt', '--no-mcp', '--no-native-dialogs')
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'library material own color driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "library material own color driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the Materials window renamed the item and listed the new name'
Ui-StopApp

# The item's file: the new name and its own color made again and carried.
$file = Get-Content -Raw $item | ConvertFrom-Json
if ($file.material.name -ne 'Shop TPU') { Ui-Fail "the file's name is '$($file.material.name)', not Shop TPU" }
Write-Host 'ok   the file has the new name'
if ($file.material.appearance -ne 'generic_tpu_x_colour' -or $file.appearance.id -ne 'generic_tpu_x_colour' -or
    $file.appearance.name -ne 'Shop TPU color') {
  Ui-Fail "the file does not carry its own color: $($file | ConvertTo-Json -Compress -Depth 6)"
}
Write-Host 'ok   the file carries its own color made again'
Write-Host 'PASS: library material own color'
