# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/TestDriver.cpp
# check-all sources: app/commands/PrimitiveCommands.cpp tools/ui-scripts/preview-transparency.mitcad-ui
# A new sphere's see-through preview without a pattern of triangles
# (mitcad#1575, GitHub mitcad/Mitcad#219) on Mesa's software renderer, as
# tools/ui-preview-transparency-test.sh does on Linux: the shared steps
# tools/ui-scripts/preview-transparency.mitcad-ui in an empty design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-preview-transparency-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-preview-transparency'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/preview-transparency.mitcad-ui'
Ui-StartApp 'preview-transparency' @('--ui=qt', '--no-mcp')
# The handle read while the app runs: without it ExitCode stays empty
# after the exit (Start-Process -PassThru).
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'preview transparency driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "preview transparency driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a sphere preview see-through without a pattern of triangles'
Ui-StopApp
Write-Host 'PASS: preview transparency'
