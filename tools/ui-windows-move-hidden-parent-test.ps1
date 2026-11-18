# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/commands/MoveCommands.cpp app/browser/BrowserPanel.cpp
# check-all sources: tools/ui-scripts/move-hidden-parent.mitcad-ui app/framework/TestDriver.cpp
# Move on a component hidden with its parent (mitcad#1995, GitHub
# mitcad/Mitcad#430), in the build VM: the shared steps
# tools/ui-scripts/move-hidden-parent.mitcad-ui (also run by
# tools/ui-move-hidden-parent-test.sh and
# tools/ui-macos-move-hidden-parent-test.sh) on the reported design of the
# private test files (MITCAD_MITCAD_REPORTED, github-430; never committed),
# unpacked into the test's folder: the component inserted under the hidden
# Fairlead:1, picked in the Browser, is moved 40 mm along X by Move/Copy.
# Skipped (77) without the design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-move-hidden-parent-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-move-hidden-parent'),
  [string]$QtBin = '')
$reportedRoot = $env:MITCAD_MITCAD_REPORTED
$zip = $null
if ($reportedRoot -and (Test-Path -LiteralPath (Join-Path $reportedRoot 'github-430'))) {
  $zip = Get-ChildItem -LiteralPath (Join-Path $reportedRoot 'github-430') -File -Filter '*.zip' |
    Sort-Object FullName | Select-Object -First 1
}
if (-not $zip) {
  Write-Host 'SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-430) is not here'
  exit 77
}
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$unpacked = Join-Path $work 'reported'
Expand-Archive -LiteralPath $zip.FullName -DestinationPath $unpacked -Force
$design = Get-ChildItem -LiteralPath $unpacked -Recurse -File -Filter '*.mitcad' | Sort-Object FullName | Select-Object -First 1
if (-not $design) { Ui-Fail 'no design in the reported zip' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/move-hidden-parent.mitcad-ui'
Ui-StartApp 'move-hidden-parent' @('--ui=qt', '--no-mcp', '--open', $design.FullName)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'move hidden parent driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "move hidden parent driver failed (exit code $exitCode)"
}
Ui-ExpectLog ': hidden with Fairlead:1' 'the rows in Fairlead:1 are hidden with it'
Ui-ExpectLog 'Move/Copy Objects: 1 component [component C3 in O1/O2/O3]' 'Move/Copy takes the hidden component'
Ui-ExpectLog 'Moved 1 components (' 'OK moves it'
Ui-ExpectLog 'TestDriver: done' 'every step passed'
Ui-StopApp
Write-Host 'PASS: Move of a component hidden with its parent'
