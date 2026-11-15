# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/TestDriver.cpp tools/cli
# A value typed while a slow preview of the value before computes drops
# that preview (mitcad#1788) in the build VM, as
# tools/ui-pipe-preview-drop-test.sh on Linux: the shared steps
# tools/ui-scripts/pipe-preview-drop.mitcad-ui on the 50 mm line that
# mitcad-cli makes (tools/ui-scripts/pipe-preview-drop.json): Pipe with the
# path picked first, its default size's preview held by the test's worker
# gate for more than two seconds, then the size 2 typed: the held preview
# is dropped, the 2 mm pipe previewed once the gate lets it, and OK makes
# it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-pipe-preview-drop-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-pipe-preview-drop'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'pipe-preview-drop.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/pipe-preview-drop.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/pipe-preview-drop.mitcad-ui'
Ui-StartApp 'pipe-preview-drop' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'pipe preview drop driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "pipe preview drop driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Pipe: dropped after' 'the slow preview is dropped for the typed size'
Ui-ExpectLog 'Pipe Section Size: 2 = ' 'the typed size is evaluated'
Ui-ExpectLog 'TestDriver: done' 'a typed size drops the slow preview of the size before'
Ui-StopApp
Write-Host 'PASS: a typed size drops a slow Pipe preview'
