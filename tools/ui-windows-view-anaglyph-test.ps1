# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewController.cpp app/OcctViewer.cpp tools/cli
# Anaglyph 3D (mitcad#1449) in the build VM, as
# tools/ui-view-anaglyph-test.sh on Linux: the shared steps
# tools/ui-scripts/view-anaglyph.mitcad-ui on the block that mitcad-cli
# makes (tools/ui-scripts/view-anaglyph.json): View > Anaglyph 3D makes the
# camera a stereo one drawn red/cyan and is saved; a click in the view
# still selects a face; Preferences > Display's stereo depth and
# convergence reach the view at once, are saved with OK and put back on
# Cancel; off, the camera is in its projection from before; Orthographic
# ends it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-view-anaglyph-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-view-anaglyph'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/view-anaglyph.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the block was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/view-anaglyph.mitcad-ui'
Ui-StartApp 'view-anaglyph' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'anaglyph driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "anaglyph driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Anaglyph 3D: a stereo camera, picks, its preferences'
Ui-StopApp
Write-Host 'PASS: Anaglyph 3D'
