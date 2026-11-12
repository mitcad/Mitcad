# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/PickList.cpp app/view/PickListView.cpp app/framework/TestDriver.cpp tools/cli
# The pick list (mitcad#1703, GitHub mitcad/Mitcad#270) in the build VM, as
# tools/ui-pick-list-test.sh on Linux: the shared steps
# tools/ui-scripts/pick-list.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/pick-list.json). A press held still on a sketch's square
# lists the profile and the block's top face behind it, nearest first and in
# words; the second row hovered highlights the face and chosen selects it,
# Ctrl adds a row, Esc and a click elsewhere close the list, and inside
# Extrude a row goes to its input.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-pick-list-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-pick-list'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'pick-list.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/pick-list.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/pick-list.mitcad-ui'
Ui-StartApp 'pick-list' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'pick list driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "pick list driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a held press lists the profile and the face behind it; rows hovered, chosen, added, closed'
Ui-StopApp
Write-Host 'PASS: pick list'
