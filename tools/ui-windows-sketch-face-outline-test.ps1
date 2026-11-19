# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp app/MainWindow.cpp app/view tools/cli
# check-all sources: core/model-api/src/sketch.rs core/sketch/src/regions.rs core/sketch/src/def.rs
# check-all sources: tools/ui-scripts/sketch-face-outline.mitcad-ui tools/ui-scripts/sketch-face-outline.json
# A sketch on a planar face starts with the face's outline, and Convert to
# Sketch Geometry takes a profile (mitcad#2051, GitHub mitcad/Mitcad#426)
# in the build VM, as tools/ui-sketch-face-outline-test.sh on Linux: the
# shared steps tools/ui-scripts/sketch-face-outline.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/sketch-face-outline.json: a
# 60 x 40 x 20 block).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-face-outline-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-face-outline'),
  [string]$QtBin = '')
# The preference's own default, on (the library turns it off unless set).
$env:MITCAD_SKETCH_PROJECT_FACE = '1'
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-face-outline.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-face-outline.mitcad-ui'
Ui-StartApp 'sketch-face-outline' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(200000)) { Ui-Fail 'sketch face outline driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch face outline driver failed (exit code $exitCode)"
}
Ui-ExpectLog "starts with the face's outline: " 'a new sketch on a face starts with its outline'
Ui-ExpectLog 'Undo: Convert to Sketch Geometry in Sketch2' 'the converted outline restored by one undo'
Ui-ExpectLog 'Preferences: project the face' 'the preference turned off'
Ui-ExpectLog 'TestDriver: done' 'outline, Convert of its profile, no Convert of ordinary curves, the preference off'
Ui-StopApp
Write-Host 'PASS: sketch face outline'
