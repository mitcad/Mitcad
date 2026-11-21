# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchBlocks.cpp app/commands/SketchCommands.cpp app/browser
# check-all sources: core/sketch core/model-api/src/sketch_blocks.rs core/model/src/document/sketch_blocks.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-blocks.mitcad-ui tools/ui-scripts/sketch-blocks.json
# Sketch blocks (mitcad#2110) in the build VM, as
# tools/ui-sketch-blocks-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-blocks.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/sketch-blocks.json): Create Block of a slot, two
# instances inserted, one placed by a coincident and an angle, the block
# edited in place and every instance following, one exploded.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-blocks-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-blocks'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'slot.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-blocks.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-blocks.mitcad-ui'
Ui-StartApp 'sketch-blocks' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(170000)) { Ui-Fail 'sketch blocks driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch blocks driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Created block Slot of ' 'Create Block made the slot a block in place'
Ui-ExpectLog 'Block instance k8 of Slot: 30.000 mm along its direction' 'the placed instance followed the edited block'
Ui-ExpectLog 'Exploded instance k7 of block Slot in Sketch1' 'Explode Block'
Ui-ExpectLog 'TestDriver: done' 'sketch blocks made, inserted, edited and exploded'
Ui-StopApp
Write-Host 'PASS: sketch blocks'
