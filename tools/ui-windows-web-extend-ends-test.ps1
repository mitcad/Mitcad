# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp core/model/src/features/rib.rs geometry/src/rib.cpp tools/cli
# A Web's Extend Ends and mitred corner (mitcad#1968) in the build VM, as
# tools/ui-web-extend-ends-test.sh on Linux: the shared steps
# tools/ui-scripts/web-extend-ends.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/web-extend-ends.json): Extend Ends checked for a
# new web, its walls reaching the walls, mitred at the corner.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-web-extend-ends-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-web-extend-ends'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'web-extend-ends.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/web-extend-ends.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/web-extend-ends.mitcad-ui'
Ui-StartApp 'web-extend-ends' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'web extend ends driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "web extend ends driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added web on 2 curve' 'the web is added'
Ui-ExpectLog 'TestDriver: done' 'Extend Ends is on for a new web; its walls reach the walls, mitred'
Ui-StopApp
Write-Host 'PASS: Extend Ends is on for a new web; its walls reach the walls, mitred'
