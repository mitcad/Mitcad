# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/selection_sets.rs tools/cli tools/ui-scripts/selection-sets.mitcad-ui
# Selection sets (mitcad#2192) in the build VM, as
# tools/ui-selection-sets-test.sh on Linux: the shared steps
# tools/ui-scripts/selection-sets.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/selection-sets.json): a body, a used sketch and a
# feature into a new set, renamed in place; the set's row selects them;
# Find in Timeline and Find in Browser from member rows; Remove from Set,
# the Delete key on a member row (the link goes, the body stays,
# mitcad#2219), Delete and Undo; the saved design checked with
# selection-sets-check.json.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-selection-sets-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-selection-sets'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'selection-sets.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/selection-sets.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$name = 'selection-sets'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${name}: driver timed out" }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "${name}: driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
Ui-StopApp
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/selection-sets-check.json') --open $design
if ($LASTEXITCODE -ne 0) { Ui-Fail "${name}: the saved design does not pass selection-sets-check.json" }
Write-Host 'PASS: a selection set created, renamed, selected, its members found and removed, deleted and undone, kept with the design'
