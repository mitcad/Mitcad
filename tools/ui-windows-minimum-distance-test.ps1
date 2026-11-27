# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MinimumDistance.cpp app/commands/InspectCommands.cpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp tools/ui-scripts/minimum-distance.mitcad-ui
# Minimum Distance (mitcad#2519) in the build VM, as
# tools/ui-minimum-distance-test.sh on Linux: the shared steps
# tools/ui-scripts/minimum-distance.mitcad-ui on the two blocks mitcad-cli
# makes in the work folder (tools/ui-scripts/minimum-distance.json).
# Inspect > Minimum Distance between Body1 and Body2 gives 5 mm, closer than
# a 6 mm clearance and clear of a 4 mm one; OK keeps it as MinDistance1,
# measured again when the blocks' parameter changes, hidden, shown, edited,
# saved (checked in the file here), deleted and undone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-minimum-distance-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-minimum-distance'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'distance-blocks.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/minimum-distance.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the two blocks were not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/minimum-distance.mitcad-ui'
Ui-StartApp 'minimum-distance' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'minimum distance driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "minimum distance driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Inspect Minimum Distance: 5.000 mm from Body1' 'Body1 to Body2: 5 mm'
Ui-ExpectLog 'clearance 6 mm: closer' 'closer than a 6 mm clearance'
Ui-ExpectLog 'Kept Minimum Distance as MinDistance1' 'kept in the document'
Ui-ExpectLog 'Minimum distance MinDistance1: 3.000 mm' 'measured again after the parameter changed'
Ui-ExpectLog 'Deleted analysis MinDistance1' 'deleted from its context menu'
Ui-ExpectLog 'TestDriver: done' 'Minimum Distance through the UI'
$saved = Get-Content -Raw $design
if (-not $saved.Contains('"type": "minimum_distance"') -or -not $saved.Contains('"name": "MinDistance1"')) {
  Ui-Fail 'no MinDistance1 minimum distance analysis in the saved file'
}
Write-Host 'ok   the analysis is in the saved file'
Ui-StopApp
Write-Host 'PASS: Minimum Distance is measured, kept, updated, saved and deleted'
