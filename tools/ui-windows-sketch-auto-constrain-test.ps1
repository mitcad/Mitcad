# SPDX-License-Identifier: MIT
# check-all sources: app/commands/AutoConstrainCommands.cpp app/commands/SketchCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/sketch/SketchController.cpp core/sketch core/solver tools/cli
# check-all sources: tools/ui-scripts/sketch-auto-constrain.mitcad-ui tools/ui-scripts/sketch-auto-constrain.json
# Auto Constrain (mitcad#2019) in the build VM, as
# tools/ui-sketch-auto-constrain-test.sh on Linux: the shared steps
# tools/ui-scripts/sketch-auto-constrain.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/sketch-auto-constrain.json): a
# rectangle of four lines without constraints ends fully constrained, the
# relations only leave 4 degrees of freedom, and OK is one undo step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-auto-constrain-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-auto-constrain'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'rectangle.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/sketch-auto-constrain.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/sketch-auto-constrain.mitcad-ui'
Ui-StartApp 'sketch-auto-constrain' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail 'sketch Auto Constrain driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "sketch Auto Constrain driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Preview Auto Constrain added: coincident 4, horizontal 2, vertical 2 (dof 4)' 'the relations only leave 4 degrees of freedom'
Ui-ExpectLog 'Added Auto Constrain' 'OK kept Auto Constrain'
Ui-ExpectLog 'TestDriver: done' 'Auto Constrain made the rectangle fully constrained in one undo step'
Ui-StopApp
Write-Host 'PASS: sketch Auto Constrain'
