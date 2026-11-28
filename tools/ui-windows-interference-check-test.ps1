# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/framework/CommandSession.cpp app/browser app/MainWindow.cpp
# check-all sources: core/model/src/document/analyses.rs tools/ui-scripts/interference-check.mitcad-ui tools/ui-scripts/interference-check-reopen.mitcad-ui
# Interference checks kept in the Analysis folder (mitcad#2664) in the build
# VM, as tools/ui-interference-check-test.sh on Linux: the shared steps
# tools/ui-scripts/interference-check.mitcad-ui on the four blocks
# mitcad-cli makes in the work folder (tools/ui-scripts/interference-check.json),
# then interference-check-reopen.mitcad-ui on the saved design. Body1 and
# Body2 kept as Interference1, every body as Interference2, both run again
# from the browser after the geometry changes; saved (checked in the file
# here), opened again, and Interference1 edited once Body2 is gone (checked
# in the file here).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-interference-check-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-interference-check'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'interference-check.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/interference-check.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the four blocks were not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
foreach ($name in @('interference-check', 'interference-check-reopen')) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$name.mitcad-ui"
  Ui-StartApp $name @('--ui=qt', '--no-mcp', '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "${name}: driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${name}: driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${name}: the shared steps"
  if ($name -eq 'interference-check') {
    Ui-ExpectLog 'Kept Interference as Interference1' 'the check of two bodies kept'
    Ui-ExpectLog 'Kept Interference as Interference2' 'the check of every body kept'
    $saved = Get-Content -Raw $design
    if (-not $saved.Contains('"type": "interference"') -or -not $saved.Contains('"name": "Interference2"')) {
      Ui-Fail 'no interference checks in the saved file'
    }
    Write-Host 'ok   both checks are in the saved file'
  } else {
    Ui-ExpectLog 'Edited analysis Interference1' 'the check kept with its new bodies'
    $saved = Get-Content -Raw $design | ConvertFrom-Json
    $bodies = (@($saved.analyses) | Where-Object { $_.name -eq 'Interference1' }).bodies -join ','
    if ($bodies -ne 'F2.b0,F6.b0') { Ui-Fail "Interference1's saved bodies: $bodies" }
    Write-Host 'ok   Interference1 saved with Body1 and Body3'
  }
  Ui-StopApp
}
Write-Host 'PASS: interference checks are kept, run again, saved and edited when a body is gone'
