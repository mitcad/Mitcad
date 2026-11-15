# SPDX-License-Identifier: MIT
# check-all sources: app/framework/MoveOccurrenceDialog.cpp app/browser/BrowserController.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/move-occurrence-group.mitcad-ui tools/ui-scripts/move-occurrence-group.json
# Move to Component on a rigid group joined to a shaft (mitcad#1847,
# GitHub mitcad/Mitcad#334) in the build VM, as
# tools/ui-move-occurrence-group-test.sh on Linux: the shared steps
# tools/ui-scripts/move-occurrence-group.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/move-occurrence-group.json): on two
# selected occurrences every parent they cannot go into says why under
# it, the joint to the shaft with what to do; on one of them the rest of
# its rigid group is offered; with the shaft selected too, all three go
# into the empty component in one step, and one Undo puts them back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-move-occurrence-group-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-occurrence-group'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'move-occurrence-group.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/move-occurrence-group.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/move-occurrence-group.mitcad-ui'
Ui-StartApp 'move-occurrence-group' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'move occurrence group driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "move occurrence group driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Move Component reason shown under Root/Holder:1: Joint1: it joins Bearing:1 to Shaft:1' 'the reason under the empty component, in words'
Ui-ExpectLog 'Move Component rigid group offered: Tube:1' 'the rest of the rigid group offered'
Ui-ExpectLog 'Moved Bearing:1, ' 'the group and the shaft moved in one step'
Ui-ExpectLog 'TestDriver: done' 'Move to Component on a rigid group'
Ui-StopApp
Write-Host 'PASS: Move to Component shows every reason and moves a rigid group as one'
