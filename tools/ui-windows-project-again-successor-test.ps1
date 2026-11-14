# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/sketch tools/cli
# Project Again's choice of the edge that takes a lost projection's
# source's place (mitcad#1794) in the build VM, as
# tools/ui-project-again-successor-test.sh on Linux: the shared steps
# tools/ui-scripts/project-again-successor.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/project-again-successor.json): the
# palette offers Project Again under the lost projection's warning, its
# panel lists the two edges in words, the pointer over a row paints its
# edge, a click chooses it, and OK keeps it without the warning.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-project-again-successor-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-project-again-successor'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'project-again-successor.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/project-again-successor.json') --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail 'project again successor fixture creation failed' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/project-again-successor.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'project-again-successor' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'Project Again successor driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Project Again successor driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'Project Again lists the edges in words, paints the hovered one and takes the chosen one'
Ui-StopApp
Write-Host 'PASS: Project Again chooses the edge in a lost source''s place'
