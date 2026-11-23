# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveTarget.cpp app/commands/MoveTarget.hpp app/framework/Command.hpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/recompute.rs core/model/src/features/moves.rs app/commands/MoveDragMotion.cpp app/commands/MoveDragMotion.hpp tools/cli tools/ui-scripts/move-nested-component.mitcad-ui tools/ui-scripts/move-nested-component.json
# Move/Copy, Move Object Components, on a component three levels deep
# picked by its Browser row with the root active (mitcad#2302) in the build
# VM, as tools/ui-move-nested-component-test.sh on Linux: the shared steps
# tools/ui-scripts/move-nested-component.mitcad-ui on the design mitcad-cli
# makes in the work folder from tools/ui-scripts/move-nested-component.json.
# The move goes into the component the picked one is placed in; 40 mm
# along X moves that part only, one undo puts it back, and two picks in
# different components are refused in words; the move made again and
# edited from the timeline opens in its own component and changes to 25 mm
# (mitcad#2305).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-move-nested-component-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-nested-component'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'move-nested-component.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-nested-component.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the design was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/move-nested-component.mitcad-ui'
Ui-StartApp 'move-nested-component' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'move nested component driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "move nested component driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Move/Copy: moving in C2 (at O1/O2), the parent of O1/O2/O3' "the move goes into the picked component's parent"
Ui-ExpectLog 'Moved 1 components (' 'the nested component moved'
Ui-ExpectLog 'Preview Move/Copy: not built: These components are placed in different components' 'picks in two components refused in words'
Ui-ExpectLog 'Edited F' 'the nested move edited from the timeline'
Ui-ExpectLog 'TestDriver: done' 'Move/Copy of a nested component'
Ui-StopApp
Write-Host 'PASS: Move/Copy of a component picked inside a sub-assembly moves it in its parent'
