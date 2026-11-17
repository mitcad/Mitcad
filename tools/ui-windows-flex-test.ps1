# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FlexCommands.cpp app/commands/FlexRoute.cpp app/framework/CommandSession.cpp app/framework/CommandSessionHover.cpp app/framework/Command.hpp app/framework/TestDriver.cpp
# check-all sources: core/model-api/src/flex.rs core/model/src/features/flex.rs core/model/src/document/flex.rs geometry/src/flex.cpp tools/ui-scripts/flex.mitcad-ui tools/ui-scripts/flex.json tools/cli
# Flex laid by clicks (mitcad#1713) in the build VM, as tools/ui-flex-test.sh
# on Linux: the shared steps tools/ui-scripts/flex.mitcad-ui on the design
# that mitcad-cli makes (tools/ui-scripts/flex.json: an upright pipe beside
# a block), in the Docked and in the Floating layout: a hose from the
# pipe's end port over the block's top and side with the routed length in
# the panel, free nodes off the model, Backspace, a double click as the
# end, a flat section's twist, Undo and Esc.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-flex-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-flex'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/flex.mitcad-ui'
foreach ($chrome in @('docked', 'floating')) {
  Write-Host "--- Flex in the $chrome layout"
  $design = Join-Path $work "flex-$chrome.mitcad"
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/flex.json') --save $design
  if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
  Ui-StartApp "flex-$chrome" @('--ui=qt', '--no-mcp', "--chrome=$chrome", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(420000)) { Ui-Fail "the flex driver timed out ($chrome)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "the flex driver failed ($chrome, exit code $exitCode)"
  }
  Ui-ExpectLog 'Flex: node 1 at an end port' "the first click starts at the pipe's end port ($chrome)"
  Ui-ExpectLog 'Panel Flex note: Length: ' "the panel shows the routed length ($chrome)"
  Ui-ExpectLog 'Added Flex as F6' "Enter added the hose over the block ($chrome)"
  Ui-ExpectLog 'Flex: last node taken back, 1 left' "Backspace took the last node back ($chrome)"
  Ui-ExpectLog 'Flex: a click on the last node ends the item at node 3' "a double click ended the item ($chrome)"
  Ui-ExpectLog 'Undo: Add Flex3' "the flat item with its first node's twist added and undone ($chrome)"
  Ui-ExpectLog 'Command Flex cancelled' "Esc cancelled ($chrome)"
  Ui-ExpectLog 'Flex route under the pointer: 2 node(s)' "the pointer moved shows the route alone ($chrome)"
  Ui-ExpectLog 'Flex: node 1 chosen for its twist' "a click on the first node made Twist that node's ($chrome)"
  Ui-ExpectLog 'Flex: node 2 at an end port' "the second pipe's end port ended the item ($chrome)"
  Ui-ExpectLog 'Edited F6' "Flex1 edited from the timeline ($chrome)"
  Ui-ExpectLog 'Flex: saved type user/' "the panel's item saved as a type ($chrome)"
  Ui-ExpectLog 'TestDriver: done' "the script ran to its end ($chrome)"
  Ui-StopApp
}
Write-Host 'PASS: UI Windows flex'
