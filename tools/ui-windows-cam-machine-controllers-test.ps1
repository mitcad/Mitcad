# SPDX-License-Identifier: MIT
# check-all sources: app/cam core/model/src/cam core/model-api/src/cam.rs core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-machine-controllers.mitcad-ui tools/ui-scripts/cam-machine-controllers.json
# The Controller list of Machines and Tools... > Edit Machine (mitcad#2119)
# on Windows, the shared input script
# tools/ui-scripts/cam-machine-controllers.mitcad-ui as
# tools/ui-cam-machine-controllers-test.sh runs it on Linux: the router's
# list has the milling dialects and no lathe dialect, the lathe's only the
# lathe dialects.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam-machine-controllers'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'machines.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam-machine-controllers.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM machine controllers fixture creation failed' }

$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam-machine-controllers.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam-machine-controllers' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'CAM machine controllers driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM machine controllers driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "the router's and the lathe's controller lists"
if ((Ui-CountLog '(?m)^CAM machine controllers: router: .*turning') -ne 0) {
  Ui-Fail "the router's Controller list offers a lathe dialect"
}
Ui-StopApp
Write-Host 'PASS: each machine''s Controller list has its kind''s dialects'
