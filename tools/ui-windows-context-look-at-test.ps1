# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowContextMenu.cpp app/MainWindow.cpp tools/cli
# Look At in the view's context menu (mitcad#1212) in the build VM, as
# tools/ui-context-look-at-test.sh on Linux: the shared steps
# tools/ui-scripts/context-look-at.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/context-look-at.json): no Look At for a
# cylindrical face; Look At for the planar top of a block in a component,
# which looks down the face's normal.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-context-look-at-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-context-look-at'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'look-at.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/context-look-at.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/context-look-at.mitcad-ui'
Ui-StartApp 'context-look-at' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'context Look At driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "context Look At driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "Look At in a planar face's context menu, not in a cylindrical one's"
Ui-StopApp
Write-Host 'PASS: Look At in the context menu'
