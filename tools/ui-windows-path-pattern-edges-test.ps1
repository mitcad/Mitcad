# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PatternCommands.cpp app/commands/CommandSupport.cpp app/framework/CommandSessionHover.cpp
# check-all sources: core/model/src/features/pattern.rs core/model/src/features/path.rs tools/cli
# Pattern on Path along a body's edges with Tangent Chain (mitcad#2105,
# GitHub mitcad/Mitcad#322) through the real UI in the build VM, as
# tools/ui-path-pattern-edges-test.sh on Linux, with the same input
# scripts: tools/ui-scripts/path-pattern-edges.mitcad-ui on the block that
# mitcad-cli makes (tools/ui-scripts/path-pattern-edges.json), one top edge
# as the path with its tangent chain; then mitcad-cli checks that the
# copies follow the rounded outline (tools/ui-scripts/path-pattern-edges-check.json).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-path-pattern-edges-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-path-pattern-edges'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'path-pattern-edges.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/path-pattern-edges.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/path-pattern-edges.mitcad-ui'
Ui-StartApp 'path-pattern-edges' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'the driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the driver failed or the application ended (exit code $exitCode)"
}
Ui-ExpectLog 'Hover chain: 5 edge(s) of F2.b0' "the edge's hover shows its tangent chain"
Ui-ExpectLog 'Added Pattern on Path' 'the pattern made along the edge'
Ui-ExpectLog 'Editing F6 with Pattern on Path' 'the pattern along edges edited'
Ui-ExpectLog 'TestDriver: done' 'Pattern on Path along a body edge with Tangent Chain'
Ui-StopApp
$check = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/path-pattern-edges-check.json') --open $design 2>&1 | Out-String
if ($LASTEXITCODE -ne 0) {
  Write-Host $check
  Ui-Fail 'the copies do not follow the outline'
}
Write-Host 'ok   the copies follow the rounded outline'
Write-Host 'PASS: Pattern on Path along a body edge with Tangent Chain'
