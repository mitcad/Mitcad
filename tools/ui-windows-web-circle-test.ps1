# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp core/model/src/features/rib.rs geometry/src/rib.cpp tools/cli
# A Web from a circle (mitcad#2364) in the build VM, as
# tools/ui-web-circle-test.sh on Linux: the shared steps
# tools/ui-scripts/web-circle.mitcad-ui on the design that mitcad-cli makes
# (tools/ui-scripts/web-circle.json): a ring wall joined to the plate, one
# body of the analytic volume; Undo takes it away.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-web-circle-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-web-circle'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'web-circle.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/web-circle.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '50'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/web-circle.mitcad-ui'
Ui-StartApp 'web-circle' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'web circle driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "web circle driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Added web on 1 curve' 'the web is added'
Ui-ExpectLog 'TestDriver: done' 'a web from a circle is a ring wall joined to the plate'
Ui-StopApp
Write-Host 'PASS: a web from a circle is a ring wall joined to the plate'
