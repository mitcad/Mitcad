# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/PickContext.cpp app/framework/CommandSession.cpp tools/cli
# A sketch region inside a body is picked before the body's face in front
# of it (mitcad#1222) in the build VM, as
# tools/ui-region-pick-inside-body-test.sh on Linux: the shared steps
# tools/ui-scripts/region-pick-inside-body.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/region-pick-inside-body.json).
# Editing the cut Extrude, a click on the square inside the block picks the
# region (logged), not the block's top face; a click on the top face where
# no region is under the cursor still picks the face.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-region-pick-inside-body-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-region-pick-inside-body'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'region-inside.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/region-pick-inside-body.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/region-pick-inside-body.mitcad-ui'
Ui-StartApp 'region-pick-inside-body' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'region pick driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "region pick driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the region inside the block picked before its face, the face where no region is'
Ui-StopApp
Write-Host 'PASS: region pick inside a body'
