# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PlasticCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp core/model/src/features/plastic.rs geometry/src/plastic.cpp geometry/src/rib.cpp tools/cli
# Plastic Feature's fields of mitcad#2511 in the build VM, as
# tools/ui-plastic-drafts-test.sh on Linux: the shared steps
# tools/ui-scripts/plastic-drafts.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/plastic-drafts.json): a lip with Follow Tangency
# and a Side Draft of its own, a boss with a rib up to the wall.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-plastic-drafts-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-plastic-drafts'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plastic-drafts.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/plastic-drafts.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/plastic-drafts.mitcad-ui'
Ui-StartApp 'plastic-drafts' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'plastic drafts driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "plastic drafts driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added plastic feature (Lip)' 'the lip is added'
Ui-ExpectLog 'Added plastic feature (Boss)' 'the boss is added'
Ui-ExpectLog 'TestDriver: done' 'the lip follows the face beside it and leans; a rib reaches the wall'
Ui-StopApp
Write-Host 'PASS: the lip follows the face beside it and leans; a rib reaches the wall'
