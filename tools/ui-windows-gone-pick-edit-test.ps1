# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/framework/CommandSessionPicks.cpp app/commands/HoleCommands.cpp tools/cli
# A selection input with a pick that is gone (mitcad#1780) in the build VM,
# as tools/ui-gone-pick-edit-test.sh on Linux: the shared steps
# tools/ui-scripts/gone-pick-edit.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/gone-pick-edit.json): editing Thread1, whose first
# face is gone, the Faces chip says one is missing, the message names the
# input, and removing the gone pick lets the edit build.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-gone-pick-edit-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-gone-pick-edit'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'gone-pick-edit.mitcad'
# The script's last step leaves Thread1 failed on purpose; the design is
# what counts.
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/gone-pick-edit.json') --save $design
if (-not (Test-Path $design)) { Ui-Fail 'gone pick fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/gone-pick-edit.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'gone-pick-edit' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'gone pick edit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "gone pick edit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the gone pick is marked, removed, and the edit builds'
Ui-StopApp
Write-Host 'PASS: a selection input marks a pick that is gone'
