# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FillCavityCommands.cpp core/model/src/features/fill_cavity.rs geometry/src/fill_cavity.cpp tools/cli
# Fill Cavity (mitcad#1818) in the build VM: the shared steps
# tools/ui-scripts/fill-cavity.mitcad-ui (also run by tools/ui-fill-cavity-test.sh
# and tools/ui-macos-fill-cavity-test.sh) on the design mitcad-cli makes
# (tools/ui-scripts/fill-cavity.json): a blind slit filled up to a plane,
# its volume added, and an open groove as the seed refused in words.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-fill-cavity-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-fill-cavity'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'fill-cavity.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/fill-cavity.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/fill-cavity.mitcad-ui'
Ui-StartApp 'fill-cavity' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(600000)) { Ui-Fail 'fill cavity driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "fill cavity driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the slit filled up to the plane, the open groove refused'
Ui-StopApp
Write-Host 'PASS: Fill Cavity fills the slit and refuses the open groove'
