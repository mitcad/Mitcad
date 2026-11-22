# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/ProfilePicks.cpp app/OcctViewer.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/sketch/src/regions.rs core/sketch/src/def.rs core/model/src/features/sketch.rs
# check-all sources: tools/ui-scripts/sketch-own-area.mitcad-ui tools/ui-scripts/sketch-own-area.json
# The face's edges do not split a shape the user closed (mitcad#2197,
# GitHub mitcad/Mitcad#544) in the build VM, as tools/ui-sketch-own-area-test.sh
# on Linux: the shared steps tools/ui-scripts/sketch-own-area.mitcad-ui on
# the design that mitcad-cli makes (tools/ui-scripts/sketch-own-area.json:
# a block and on its top face a circle across the face's edge, and a
# rectangle past the face with two circles inside it).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-own-area-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-own-area'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'own-area.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-own-area.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-own-area.mitcad-ui'
Ui-StartApp 'sketch-own-area' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(200000)) { Ui-Fail 'sketch own area driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch own area driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Extrude Profiles: 1 profile [profile r{c1}' 'one click takes the whole disc across the face edge'
Ui-ExpectLog 'TestDriver: done' 'the whole cylinder, and the plate without its holes'
Ui-StopApp
Write-Host 'PASS: sketch own area'
