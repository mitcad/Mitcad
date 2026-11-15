# SPDX-License-Identifier: MIT
# check-all sources: app/view/DatumHandles.cpp app/view/DatumHandles.hpp app/MainWindowOrigin.cpp app/MainWindow.cpp
# check-all sources: app/framework/ModelShapes.cpp app/framework/TestDriver.cpp core/model/src/features/construction
# check-all sources: core/model/src/document/datums.rs core/model-api/src/analysis.rs tools/cli
# Construction planes drawn at the size and shape of what they come from
# (mitcad#1827, GitHub mitcad/Mitcad#321) in the build VM, as
# tools/ui-construction-size-test.sh on Linux: the shared steps
# tools/ui-scripts/construction-size.mitcad-ui on a design that mitcad-cli
# makes (tools/ui-scripts/construction-size.json): a plane saved with a
# size of its own opens unchanged; a plane off a long narrow face is a
# rectangle over it, one over a small face is small; a corner handle's
# drag stores a size, Reset Size in its context menu brings the fitted
# rectangle back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-construction-size-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-construction-size'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}

$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'construction-size.mitcad'
$output = & $Cli 'run' (Join-Path $PSScriptRoot 'ui-scripts/construction-size.json') '--save' $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli run construction-size.json: $($output -join ' / ')" }

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_LOG_PICKS = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/construction-size.mitcad-ui'
Ui-StartApp 'construction-size' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'construction size driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "construction size driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Datum F5 drawn 60 x 60 mm (display_size)' 'a plane saved with a size of its own opens unchanged'
Ui-ExpectLog 'Datum F3 drawn 220 x 30 mm (fitted)' 'a plane off a long narrow face is a rectangle over it'
Ui-ExpectLog 'Datum F4 drawn 16 x 16 mm (fitted)' 'a plane over a small face is small'
Ui-ExpectLog 'Datum handle F3 resized to ' "a corner handle's drag stored a size"
Ui-ExpectLog 'Datum handle F3 size reset' "Reset Size in the handle's menu"
Ui-ExpectLog 'TestDriver: done' 'construction planes fitted to their faces, resized and reset'
Ui-StopApp
Write-Host 'PASS: construction size'
