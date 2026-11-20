# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/framework/InputPicks.cpp app/framework/CommandSessionPicks.cpp app/framework/TestDriver.cpp app/commands/PatternCommands.cpp app/commands/FilletCommands.cpp tools/cli
# A selection input's picks listed by name on demand, each highlighted in
# the view on hover (mitcad#2064, GitHub mitcad/Mitcad#473) in the build
# VM, as tools/ui-input-picks-test.sh on Linux: the shared steps
# tools/ui-scripts/input-picks.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/input-picks.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-input-picks-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-input-picks'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'input-picks.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/input-picks.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/input-picks.mitcad-ui'
Ui-StartApp 'input-picks' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'input picks driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "input picks driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Panel Pattern on Path pick bodies hovered: 2. Body2' 'a row under the pointer names its pick'
Ui-ExpectLog 'TestDriver: done' 'the picks are listed by name on demand and highlighted on hover'
Ui-StopApp
Write-Host 'PASS: the picks are listed by name on demand and highlighted on hover'
