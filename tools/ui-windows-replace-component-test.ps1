# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ReplaceComponentDialog.cpp app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: tools/ui-scripts/replace-component.mitcad-ui tools/cli
# Replace Component (mitcad#2640) in the build VM, as
# tools/ui-replace-component-test.sh on Linux: the shared steps
# tools/ui-scripts/replace-component.mitcad-ui on the designs mitcad-cli
# makes in the work folder (replace-component-bracket.json,
# replace-component-bracket-v2.json, and the holder in their folder,
# replace-component-holder.json). Replace Component... from the browser row
# with Bracket_v2.mitcad shows the replacement in all occurrences with the
# joint kept, OK keeps it as one undo step and Undo takes it back; This one
# from the other row shows it there alone and Cancel takes it back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-replace-component-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-replace-component'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/replace-component-bracket.json') --save (Join-Path $work 'Bracket.mitcad') 2>&1
$status = $LASTEXITCODE
if ($status -eq 0) {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/replace-component-bracket-v2.json') --save (Join-Path $work 'Bracket_v2.mitcad') 2>&1
  $status = $LASTEXITCODE
}
if ($status -ne 0) { Ui-Fail "mitcad-cli: the brackets were not made: $($output -join ' / ')" }
# The holder made in the brackets' folder: its link is Bracket.mitcad.
Push-Location $work
try {
  $output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/replace-component-holder.json') --save (Join-Path $work 'holder.mitcad') 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the holder was not made: $($output -join ' / ')" }
} finally {
  Pop-Location
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/replace-component.mitcad-ui'
Ui-StartApp 'replace-component' @('--ui=qt', '--no-mcp', '--no-recovery', '--open', (Join-Path $work 'holder.mitcad'))
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'replace component driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "replace component driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Replace Component: Bracket replaced by Bracket in 2 occurrences, 0 unmatched' 'all occurrences replaced, the joint kept, the name kept'
Ui-ExpectLog 'Replace Component canceled' 'This one previewed and canceled'
Ui-ExpectLog 'TestDriver: done' 'replacing a component'
Ui-StopApp
Write-Host 'PASS: a component replaced in all its occurrences and in one'
