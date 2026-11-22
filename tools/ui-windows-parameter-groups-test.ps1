# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp tools/cli
# check-all sources: tools/ui-scripts/parameter-groups.mitcad-ui tools/ui-scripts/parameter-groups.json
# Groups of user parameters in Edit Parameters (mitcad#2180) in the build
# VM, as tools/ui-parameter-groups-test.sh on Linux: the shared steps
# tools/ui-scripts/parameter-groups.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/parameter-groups.json): a group made, two
# parameters moved into it by the context menu, the group renamed in
# place, collapsed (kept when the dialog opens again) and deleted, its
# parameters back under User Parameters.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-parameter-groups-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-parameter-groups'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'parameter-groups.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/parameter-groups.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/parameter-groups.mitcad-ui'
Ui-StartApp 'parameter-groups' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'the parameter groups driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the parameter groups driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Parameter group added: Cabinet' 'New Group makes a group'
Ui-ExpectLog 'Parameters moved to group Cabinet: width, depth' 'the context menu moves two parameters into it'
Ui-ExpectLog 'Parameter group Cabinet renamed to Table' 'the group is renamed in place'
Ui-ExpectLog 'Parameters group Table collapsed' 'the group collapses'
Ui-ExpectLog 'Parameter group Table deleted: width, depth moved to User Parameters' 'a deleted group leaves its parameters'
Ui-ExpectLog 'TestDriver: done' 'the parameters are grouped, moved, renamed, collapsed and ungrouped'
Ui-StopApp
Write-Host 'PASS: Edit Parameters groups'
