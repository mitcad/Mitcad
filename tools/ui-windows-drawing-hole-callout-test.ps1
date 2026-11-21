# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawings.rs core/model-drawing/src
# check-all sources: tools/ui-scripts/drawing-hole-callout.mitcad-ui tools/ui-scripts/drawing-hole-callout.json
# A drawing's own hole callout (mitcad#2054) in the build VM, as
# tools/ui-drawing-hole-callout-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-hole-callout.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/drawing-hole-callout.json): Hole
# Callout on a counterbored hole writes its lines; Text After typed in its
# panel goes after them; the drill made larger, the callout follows.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-hole-callout-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-hole-callout'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'holed.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-hole-callout.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-hole-callout.mitcad-ui'
Ui-StartApp 'drawing-hole-callout' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'hole callout driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "hole callout driver failed (exit code $exitCode)"
}
Ui-ExpectLog '"type":"hole_callout"' 'the hole callout asked for'
Ui-ExpectLog 'TestDriver: done' 'the callout written, its own text after it, the hole followed'
Ui-StopApp
Write-Host 'PASS: drawing hole callout'
