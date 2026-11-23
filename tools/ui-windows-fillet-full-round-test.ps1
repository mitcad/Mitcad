# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp geometry/src/full_round.cpp app/framework/TestDriver.cpp tools/cli
# Fillet's Full Round (mitcad#2279) in the build VM, as
# tools/ui-fillet-full-round-test.sh on Linux: the shared steps
# tools/ui-scripts/fillet-full-round.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/fillet-full-round.json), a block whose top lies
# between a vertical side and a bevel. Choosing Full Round in Radius Type
# hides Radius; the top face clicked into Edges is refused, the status bar
# saying to pick the face's two edges; its two edges picked, OK adds the
# full round.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fillet-full-round-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fillet-full-round'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fillet-full-round.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/fillet-full-round.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fillet-full-round.mitcad-ui'
Ui-StartApp 'fillet-full-round' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Fillet full round driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Fillet full round driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'not taken: Full Round takes the two edges of the face it rounds away' 'the face is refused, the status bar saying to pick its two edges'
Ui-ExpectLog 'Fillet Edges: 2 edges' 'the two edges are taken'
Ui-ExpectLog 'Added Fillet' 'OK adds the full round'
Ui-ExpectLog 'TestDriver: done' 'Full Round hides Radius and builds'
Ui-StopApp
Write-Host 'PASS: Full Round hides Radius, refuses a face and builds'
