# SPDX-License-Identifier: MIT
# check-all sources: app/commands/HoleCommands.cpp core/model/src/document/thread_fit.rs tools/cli
# The Thread panel's fit of a hole too small for the thread (mitcad#1632)
# in the build VM, as tools/ui-thread-fit-test.sh on Linux: the shared
# steps tools/ui-scripts/thread-fit.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/thread-fit.json): M8x1.25 on a 5 mm hole warns,
# the panel's note says so and its button fits the hole; OK adds the
# thread and the fit as one undo step, and Undo takes back both.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-thread-fit-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-thread-fit'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'thread-fit.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/thread-fit.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/thread-fit.mitcad-ui'
Ui-StartApp 'thread-fit' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'thread fit driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "thread fit driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Thread panel: the hole fitted to the thread, one undo step'
Ui-StopApp
Write-Host 'PASS: Thread panel fits the hole to the thread'
