# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp app/MainWindowSheet.cpp app/MainWindow.cpp app/framework/CommandSession.cpp core/model/src/sheet core/model/src/document/flange_profile.rs tools/cli
# A Flange's profile sketch edited from its panel (mitcad#1648, GitHub
# mitcad/Mitcad#237) in the build VM, as tools/ui-sheet-flange-profile-test.sh
# on Linux: the shared steps tools/ui-scripts/sheet-flange-profile.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/sheet-flange-profile.json): Edit Profile makes the
# profile sketch on the wall and opens it while the panel waits, Finish
# Sketch brings the panel back, OK adds the flange with its profile after
# the sketch in the timeline, and the flange edited again opens the same
# sketch.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sheet-flange-profile-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sheet-flange-profile'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'sheet-flange-profile.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-profile.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sheet-flange-profile.mitcad-ui'
Ui-StartApp 'sheet-flange-profile' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'sheet flange profile driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sheet flange profile driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Flange set aside while a sketch is edited' 'the panel waited while the profile sketch was edited'
Ui-ExpectLog 'Flange back from the sketch Sketch2' 'Finish Sketch brought the panel back'
Ui-ExpectLog 'Added Flange as F6' 'the flange was added with its profile'
Ui-ExpectLog 'TestDriver: done' 'the profile is edited from the Flange panel and linked in the timeline'
Ui-StopApp
Write-Host 'PASS: the Flange profile is edited from the panel and linked in the timeline'
