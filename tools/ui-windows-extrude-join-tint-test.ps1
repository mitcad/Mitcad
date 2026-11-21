# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/CommandSession.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/TestDriver.cpp tools/cli
# An Extrude Join's target body in the command's look (mitcad#2069) in the
# build VM, as tools/ui-extrude-join-tint-test.sh on Linux: the shared steps
# tools/ui-scripts/extrude-join-tint.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/extrude-join-tint.json): the Join's target tinted
# faintly, nearer its own color than the cut's red, and the added material
# blue.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-join-tint-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-join-tint'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'tint.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-join-tint.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-join-tint.mitcad-ui'
Ui-StartApp 'extrude-join-tint' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'extrude join tint driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "extrude join tint driver failed (exit code $exitCode)"
}
# (Ui-ExpectLog matches fixed text: these two are patterns.)
foreach ($want in @(@('off [0-9]+ from .* and [0-9]+ from ', 'the target body nearer its own color than the red'),
    @('blue [0-9]+ over red ', 'the added material blue'))) {
  if ("$(Ui-LogText)" -match "TestDriver: view point pixel [^\r\n]*, $($want[0])") { Write-Host "ok   $($want[1])" }
  else { Ui-Fail "$($want[1]): no view point pixel line '$($want[0])'" }
}
Ui-ExpectLog 'TestDriver: done' 'the Join target tint'
Ui-StopApp
Write-Host 'PASS: extrude join tint'
