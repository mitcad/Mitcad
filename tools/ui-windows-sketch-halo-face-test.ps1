# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/cli
# Where fully constrained curves get their halo (mitcad#2034), through the
# test driver (tools/ui-scripts/sketch-halo-face.mitcad-ui, shared with the
# Linux and macOS tests): on a dark background always, on a light one only
# over a dark face, following the body's and the face's appearances and
# their undo and redo, read from the "Sketch halos:" log.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-halo-face'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'halo-face.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-halo-face.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'halo fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-halo-face.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'sketch-halo-face' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'halo driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "halo driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the halos follow the background and the appearances'
Ui-StopApp
Write-Host 'PASS: sketch halos over a dark face'
