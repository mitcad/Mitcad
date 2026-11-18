# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/hidden-parent-insert.mitcad-ui app/framework/TestDriver.cpp
# Inserting into a component under a hidden parent (mitcad#1995, GitHub
# mitcad/Mitcad#430) in the build VM, as
# tools/ui-hidden-parent-insert-test.sh on Linux: the shared steps
# tools/ui-scripts/hidden-parent-insert.mitcad-ui on the designs mitcad-cli
# makes in the work folder (tools/ui-scripts/hidden-parent-insert.json and
# hidden-parent-insert-part.json). Fairlead:1 hidden, the rows in it have
# the dimmed closed eye; with Post:1 active, an insert and New Component
# show Fairlead:1 in their undo step and say so in the status bar; a click
# on Post:1's dimmed eye shows Fairlead:1.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-hidden-parent-insert-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-hidden-parent-insert'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$part = Join-Path $work 'part.mitcad'
$design = Join-Path $work 'fairlead.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/hidden-parent-insert-part.json') --save $part 2>&1
$status = $LASTEXITCODE
if ($status -eq 0) {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/hidden-parent-insert.json') --save $design 2>&1
  $status = $LASTEXITCODE
}
if ($status -ne 0) { Ui-Fail "mitcad-cli: the designs were not made: $($output -join ' / ')" }
# The script with the inserted design's path (forward slashes: a JSON text).
$driverScript = Join-Path $work 'hidden-parent-insert.mitcad-ui'
$steps = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/hidden-parent-insert.mitcad-ui')
Set-Content -Encoding ascii $driverScript ($steps.Replace('@PART@', ($part -replace '\\', '/')))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = $driverScript
Ui-StartApp 'hidden-parent-insert' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'hidden parent insert driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "hidden parent insert driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Browser eye Root/Fairlead:1/Post:1: hidden with Fairlead:1' 'Post:1''s eye is hidden with Fairlead:1'
Ui-ExpectLog 'Hidden parents shown: Fairlead:1 (by insert_component)' 'the insert shows Fairlead:1'
Ui-ExpectLog 'Fairlead:1 was hidden; shown so the new component can be seen.' 'the status bar says so'
Ui-ExpectLog 'Hidden parents shown: Fairlead:1 (by create_component)' 'New Component shows Fairlead:1'
Ui-ExpectLog 'Visibility Root/Fairlead:1/Post:1: shown with its parents (O1)' 'Post:1''s dimmed eye shows Fairlead:1'
Ui-ExpectLog 'TestDriver: done' 'inserting under a hidden parent'
Ui-StopApp
Write-Host 'PASS: inserting under a hidden parent shows it and says so'
