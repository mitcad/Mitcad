# SPDX-License-Identifier: MIT
# check-all sources: app/files/FileDialogs.cpp app/files/MainWindowFiles.cpp app/files/MainWindowLinks.cpp
# check-all sources: app/browser/BrowserController.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/link-parts.mitcad-ui
# Linked parts of a design (mitcad#2130, GitHub mitcad/Mitcad#479) in the
# build VM, as tools/ui-link-parts-test.sh on Linux: the shared steps
# tools/ui-scripts/link-parts.mitcad-ui on the designs mitcad-cli makes in
# the work folder (tools/ui-scripts/link-parts-source.json and
# link-parts-holder.json). Insert Component > Linked > Choose parts... with
# Bracket:1 ticked links its body alone; Choose Linked Parts... from the
# linked component's browser row adds Bracket:2, one undo step, and Undo
# takes it back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-link-parts-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-link-parts'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$source = Join-Path $work 'link-parts-source.mitcad'
$holder = Join-Path $work 'holder.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/link-parts-source.json') --save $source 2>&1
$status = $LASTEXITCODE
if ($status -eq 0) {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/link-parts-holder.json') --save $holder 2>&1
  $status = $LASTEXITCODE
}
if ($status -ne 0) { Ui-Fail "mitcad-cli: the designs were not made: $($output -join ' / ')" }
# The script with the linked design's path (forward slashes, as Qt's file
# dialog takes them too).
$driverScript = Join-Path $work 'link-parts.mitcad-ui'
$steps = Get-Content -Raw (Join-Path $PSScriptRoot 'ui-scripts/link-parts.mitcad-ui')
Set-Content -Encoding ascii $driverScript ($steps.Replace('@SOURCE@', ($source -replace '\\', '/')))
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = $driverScript
Ui-StartApp 'link-parts' @('--ui=qt', '--no-mcp', '--open', $holder)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'link parts driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "link parts driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Insert Component: linked, chosen parts' 'Linked > Choose parts... chosen'
Ui-ExpectLog 'Linked parts of link-parts-source: Bracket:1' 'the insert links Bracket:1 alone'
Ui-ExpectLog 'Choose Parts: O1, O2' 'Choose Linked Parts... ticks Bracket:2 too'
Ui-ExpectLog 'TestDriver: done' 'linking chosen parts of a design'
Ui-StopApp
Write-Host 'PASS: chosen parts of a design are linked and chosen again'
