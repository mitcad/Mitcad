# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/framework/DisplayMeshes.cpp app/framework/TestDriver.cpp geometry/io tools/cli
# A Join made in the Extrude panel draws every face of the joined body
# (GitHub #126, mitcad#1243) in the build VM, as
# tools/ui-join-mesh-reuse-test.sh on Linux: the shared steps
# tools/ui-scripts/join-mesh-reuse.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/join-mesh-reuse.json): all 8 faces of the joined
# body meshed where the view draws them, the plate's top pickable.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-join-mesh-reuse-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-join-mesh-reuse'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'join-mesh-reuse.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/join-mesh-reuse.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/join-mesh-reuse.mitcad-ui'
Ui-StartApp 'join-mesh-reuse' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Join mesh reuse driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Join mesh reuse driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: shown body F2.b0: 8 of 8 faces meshed on their faces' 'every face of the joined body is meshed on its face'
Ui-ExpectLog 'TestDriver: done' 'a Join made in the panel draws the top it took from the plate'
Ui-StopApp
Write-Host 'PASS: Join keeps the reused faces drawn'
