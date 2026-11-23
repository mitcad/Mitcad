# SPDX-License-Identifier: MIT
# check-all sources: app/cam core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/cam tools/cli
# check-all sources: tools/ui-scripts/cam-lathe-machine.mitcad-ui tools/ui-scripts/cam-lathe-machine.json
# A lathe made in Machines and Tools... (mitcad#2230) on Windows, the shared
# input script tools/ui-scripts/cam-lathe-machine.mitcad-ui as
# tools/ui-cam-lathe-machine-test.sh runs it on Linux: New Machine... >
# Lathe and its Edit Machine..., a turning tool and a parting tool, a lathe
# setup, Turn Facing and Generate, without any lathe in the design
# beforehand.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam-lathe-machine'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam-lathe-machine.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam-lathe-machine.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM lathe machine fixture creation failed' }

$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam-lathe-machine.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam-lathe-machine' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'CAM lathe machine driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM lathe machine driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a lathe made in Machines and Tools, its tools, a lathe setup, Turn Facing, Generate'
Ui-StopApp
Write-Host 'PASS: a lathe made in Machines and Tools'
