# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DraftAnalysis.cpp app/commands/InspectCommands.cpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp tools/ui-scripts/draft-analysis.mitcad-ui
# Draft Analysis (mitcad#2166, GitHub mitcad/Mitcad#535) in the build VM, as
# tools/ui-draft-analysis-test.sh on Linux: the shared steps
# tools/ui-scripts/draft-analysis.mitcad-ui on the drafted part mitcad-cli
# makes in the work folder (tools/ui-scripts/draft-analysis.json).
# Inspect > Draft Analysis gives the three classes' areas pulled along +Z,
# then at 3 degrees, flipped and pulled along Plane1 picked in the browser;
# OK keeps it as Draft1, which is hidden, shown, edited, saved (checked in
# the file here), deleted and undone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-draft-analysis-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-draft-analysis'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'draft-part.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/draft-analysis.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the drafted part was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/draft-analysis.mitcad-ui'
Ui-StartApp 'draft-analysis' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'draft analysis driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "draft analysis driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Inspect Draft Analysis: pull (0, 0, 1), angle 1: Body1 along 1249.4 mm^2' 'pulled along +Z at 1 degree'
Ui-ExpectLog 'Kept Draft Analysis as Draft1' 'kept in the document'
Ui-ExpectLog 'Deleted analysis Draft1' 'deleted from its context menu'
Ui-ExpectLog 'TestDriver: done' 'Draft Analysis through the UI'
$saved = Get-Content -Raw $design
if (-not $saved.Contains('"type": "draft"') -or -not $saved.Contains('"name": "Draft1"')) {
  Ui-Fail 'no Draft1 draft analysis in the saved file'
}
Write-Host 'ok   the analysis is in the saved file'
Ui-StopApp
Write-Host 'PASS: Draft Analysis is computed, kept, saved and deleted'
