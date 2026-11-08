# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionPreviewPicks.cpp geometry/src/item_origin.cpp geometry/src/dressup_check.cpp app/framework/TestDriver.cpp tools/cli
# A pick on Fillet's preview names the edge before the fillet it comes from
# (mitcad#1458, GitHub mitcad/Mitcad#177) in the build VM, as
# tools/ui-fillet-preview-pick-test.sh on Linux: the shared steps
# tools/ui-scripts/fillet-preview-pick.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/fillet-preview-pick.json): Fillet1,
# rounding a block's two end foot edges on a plate, edited from the
# timeline; the front's foot edge between the roundings' ends, clicked in
# the preview without Ctrl, joins Edges, and the preview of the three
# roundings warns of nothing hanging out.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fillet-preview-pick-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fillet-preview-pick'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fillet-preview-pick.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/fillet-preview-pick.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-preview-pick.mitcad-ui'
Ui-StartApp 'fillet-preview-pick' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Fillet preview pick driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Fillet preview pick driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Editing F3 with Fillet' 'Fillet1 opens for editing with its preview'
Ui-ExpectLog 'Fillet Edges: 3 edges' 'the foot edge the preview cut short joins Edges without Ctrl'
Ui-ExpectLog 'Edited F3' 'OK keeps the three roundings'
Ui-ExpectLog 'TestDriver: done' 'a pick on the preview names the edge before the fillet'
Ui-StopApp
Write-Host 'PASS: a pick on the preview names the edge before the fillet'
