# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowFiles.cpp app/browser/BrowserController.cpp app/commands/PlasticCommands.cpp app/resources/layout/default.json tools/cli
# The Manage tab's Recompute All, Units and Plastic Rules (mitcad#1982)
# in the build VM, as tools/ui-manage-commands-test.sh on Linux: the
# shared steps tools/ui-scripts/manage-commands.mitcad-ui on two boxes
# that mitcad-cli makes (tools/ui-scripts/manage-commands.json), each
# command run from its group's menu on the MANAGE tab.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-manage-commands-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-manage-commands'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'boxes.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/manage-commands.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/manage-commands.mitcad-ui'
Ui-StartApp 'manage-commands' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'Manage commands driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Manage commands driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Recompute All: 2 features, no undo step' 'Recompute All from Manage > Design'
Ui-ExpectLog 'Units dialog opened: mm' 'Units from Manage > Design'
Ui-ExpectLog 'Added plastic feature (Plastic Rule)' 'Plastic Rules from Manage > Rules'
Ui-ExpectLog 'TestDriver: done' 'the Manage tab runs Recompute All, Units and Plastic Rules'
Ui-StopApp
Write-Host 'PASS: the Manage tab commands'
