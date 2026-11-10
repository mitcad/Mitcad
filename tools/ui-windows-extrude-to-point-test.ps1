# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp app/commands/ExtrudeCommands.cpp
# check-all sources: app/commands/HoleCommands.cpp app/commands/CommandSupport.cpp tools/cli
# Editing a component's features whose To Object or Start Object is a
# sketch point or line (mitcad#1599) in the build VM, as the component part
# of tools/ui-extrude-to-point-test.sh on Linux: the shared driver script
# tools/ui-scripts/extrude-to-point-component.mitcad-ui on the design
# mitcad-cli makes from tools/ui-scripts/extrude-to-point-component.json.
# Editing an extrusion up to a sketch point, a hole down to one and an
# extrusion from a sketch line keeps the loaded object in Part:1 (O1),
# previews, and is not refused as another occurrence's.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-extrude-to-point-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-extrude-to-point'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}

$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'component.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/extrude-to-point-component.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) {
  Ui-Fail "mitcad-cli: $($output -join ' / ')"
}
Write-Host 'ok   the component design is made'

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/extrude-to-point-component.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'extrude-to-point' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'the component edit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the component edit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'a component''s loaded point and line kept by the edits'
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Ui-Finish 'UI Windows extrude to point test'
