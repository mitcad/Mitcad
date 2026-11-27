# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MinRadiusAnalysis.cpp app/commands/InspectCommands.cpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp tools/ui-scripts/min-radius.mitcad-ui
# Minimum Radius (mitcad#2518) in the build VM, as tools/ui-min-radius-test.sh
# on Linux: the shared steps tools/ui-scripts/min-radius.mitcad-ui on the
# rounded block mitcad-cli makes in the work folder
# (tools/ui-scripts/min-radius.json). Inspect > Minimum Radius marks the
# smallest radius, 2 mm; a Radius Limit of 11 mm takes all three roundings,
# 4 mm the 2 mm one alone; OK keeps it as MinRadius1, which is hidden, shown,
# edited, saved (checked in the file here), deleted and undone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-min-radius-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-min-radius'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'min-radius-part.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/min-radius.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the rounded block was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/min-radius.mitcad-ui'
Ui-StartApp 'min-radius' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'minimum radius driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "minimum radius driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Inspect Minimum Radius: smallest 2.000 mm on Body1' 'the smallest radius, 2 mm'
Ui-ExpectLog 'under 4 mm: 1 face(s), 62.8 mm^2' 'a limit of 4 mm takes the 2 mm rounding alone'
Ui-ExpectLog 'Kept Minimum Radius as MinRadius1' 'kept in the document'
Ui-ExpectLog 'Deleted analysis MinRadius1' 'deleted from its context menu'
Ui-ExpectLog 'TestDriver: done' 'Minimum Radius through the UI'
$saved = Get-Content -Raw $design
if (-not $saved.Contains('"type": "min_radius"') -or -not $saved.Contains('"name": "MinRadius1"')) {
  Ui-Fail 'no MinRadius1 minimum radius analysis in the saved file'
}
Write-Host 'ok   the analysis is in the saved file'
Ui-StopApp
Write-Host 'PASS: Minimum Radius is computed, kept, saved and deleted'
