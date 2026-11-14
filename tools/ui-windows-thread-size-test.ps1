# SPDX-License-Identifier: MIT
# check-all sources: app/commands/HoleCommands.cpp core/model/data/threads.json tools/cli
# The Thread panel's size from a picked face (mitcad#1781) in the build
# VM, as tools/ui-thread-size-test.sh on Linux: the shared steps
# tools/ui-scripts/thread-size.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/thread-size.json): an edited M60x3 thread keeps
# its size when its face is picked again, a size chosen by hand stays when
# a 65 mm shaft is picked (the fit warning shows), and a 60 mm shaft with
# nothing chosen suggests M60x5.5 6g.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-thread-size-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-thread-size'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'thread-size.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/thread-size.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/thread-size.mitcad-ui'
Ui-StartApp 'thread-size' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'thread size driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "thread size driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Thread panel: a chosen or edited size stays, the nearest size suggested'
Ui-StopApp
Write-Host 'PASS: Thread panel suggests the size only while none is chosen'
