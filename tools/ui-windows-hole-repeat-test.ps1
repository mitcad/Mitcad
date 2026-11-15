# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/Command.hpp app/commands/HoleCommands.cpp
# check-all sources: app/commands/HoleSizes.cpp tools/cli tools/ui-scripts/hole-repeat.mitcad-ui
# Repeat runs a value's reaction with the repeated value (mitcad#1844) and
# keeps typed only what was typed (mitcad#1845) in the build VM, as
# tools/ui-hole-repeat-test.sh on Linux: the shared steps
# tools/ui-scripts/hole-repeat.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/hole-repeat.json): a tapped countersink Hole with
# a custom thread of 12 mm; Enter repeats it, its sizes filled for 12 mm
# before the panel is there, and its countersink, not typed at the OK,
# follows a new Major Diameter; one typed stays typed after Repeat.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-hole-repeat-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-hole-repeat'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'hole-repeat.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/hole-repeat.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/hole-repeat.mitcad-ui'
Ui-StartApp 'hole-repeat' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'hole repeat driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "hole repeat driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Panel Hole repeated: ' 'Enter repeated Hole with its last settings'
Ui-ExpectLog 'TestDriver: done' 'Repeat of Hole: the sizes follow the thread, a typed one stays typed'
Ui-StopApp
Write-Host 'PASS: Repeat of Hole with a custom thread'
