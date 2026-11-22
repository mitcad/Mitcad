# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PrimitiveCommands.cpp app/commands/PlaceInputs.cpp
# check-all sources: core/model/src/features/primitive.rs geometry/src/primitive.cpp
# check-all sources: tools/ui-scripts/primitive-cone.mitcad-ui
# Cone (mitcad#2239) on Mesa's software renderer, as
# tools/ui-primitive-cone-test.sh does on Linux: the shared steps
# tools/ui-scripts/primitive-cone.mitcad-ui in an empty design (a cone on
# XY, made a frustum; volumes and faces checked).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-primitive-cone-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-primitive-cone'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_LOG_VOLUMES = '1'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/primitive-cone.mitcad-ui'
Ui-StartApp 'primitive-cone' @('--ui=qt', '--no-mcp')
# The handle read while the app runs: without it ExitCode stays empty
# after the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'primitive cone driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "primitive cone driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a cone placed on XY and made a frustum, its volumes and faces'
Ui-StopApp
Write-Host 'PASS: primitive cone'
