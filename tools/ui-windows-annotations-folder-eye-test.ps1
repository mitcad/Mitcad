# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindowMeasurements.cpp app/view/MeasurementOverlay.cpp app/framework/TestDriver.cpp core/model/src/document/display.rs tools/cli tools/ui-scripts/annotations-folder-eye.mitcad-ui tools/ui-scripts/annotations-folder-eye-reopen.mitcad-ui
# The Browser's Annotations folder's eye (mitcad#2041) in the build VM, as
# tools/ui-annotations-folder-eye-test.sh on Linux: the shared steps
# tools/ui-scripts/annotations-folder-eye.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/annotations-folder-eye.json), then
# annotations-folder-eye-reopen.mitcad-ui on the saved design. The eye
# hides every annotation (none drawn, none picked, the row greyed), one
# undo step; saved hidden, the design keeps it
# (annotations-folder-eye-check.json) and opens with none drawn; the eye
# shows the same two again (annotations-folder-eye-shown-check.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-annotations-folder-eye-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-annotations-folder-eye'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'annotations-folder-eye.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/annotations-folder-eye.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$phases = @(
  @{ Script = 'annotations-folder-eye'; Check = 'annotations-folder-eye-check' },
  @{ Script = 'annotations-folder-eye-reopen'; Check = 'annotations-folder-eye-shown-check' })
foreach ($phase in $phases) {
  $name = $phase.Script
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
  Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${name}: driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${name}: driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
  Ui-StopApp
  & $Cli run (Join-Path $PSScriptRoot "ui-scripts/$($phase.Check).json") --open $design
  if ($LASTEXITCODE -ne 0) { Ui-Fail "${name}: the saved design does not pass $($phase.Check).json" }
}
Write-Host 'PASS: the Annotations folder''s eye hides and shows the annotations, kept with the design'
