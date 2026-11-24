# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing_templates.rs core/model-drawing/src core/model/src/drawing
# check-all sources: tools/ui-scripts/drawing-blank-sheet.mitcad-ui tools/ui-scripts/drawing-blank-sheet.json
# Blank chosen in Sheet... (mitcad#2325) in the build VM, as
# tools/ui-drawing-blank-sheet-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-blank-sheet.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/drawing-blank-sheet.json: an ISO
# drawing with a view): Blank hides the title block's fields and the sheet
# then shows no frame and no title block; ISO chosen again brings them
# back with the title kept.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-blank-sheet-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-blank-sheet'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'blank.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-blank-sheet.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-blank-sheet.mitcad-ui'
Ui-StartApp 'drawing-blank-sheet' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail 'blank sheet driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "blank sheet driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Drawing sheet dialog title block fields: hidden' 'Blank hid the title block fields'
Ui-ExpectLog 'Drawing sheet border: frame no, title block no' 'the Blank sheet shows no frame and no title block'
Ui-ExpectLog 'TestDriver: done' 'ISO brought the frame and the block back with the title kept'
Ui-StopApp
Write-Host 'PASS: drawing Blank sheet'
