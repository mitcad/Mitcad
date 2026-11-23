# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp app/framework/Command.hpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/constraint-apply.mitcad-ui tools/ui-scripts/constraint-apply.json
# Constraint, one assembly constraint at a time (mitcad#2293), in the build
# VM, as tools/ui-constraint-apply-test.sh on Linux: the shared steps
# tools/ui-scripts/constraint-apply.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/constraint-apply.json): an align with an offset,
# Apply (the panel open again with its type), an insert, OK: two
# constraint features, each its own undo step; one edited in the
# Constraint panel; Undo takes them back one at a time.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-constraint-apply-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-constraint-apply'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'constraint-apply.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/constraint-apply.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/constraint-apply.mitcad-ui'
Ui-StartApp 'constraint-apply' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'constraint apply driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "constraint apply driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Panel Constraint Apply at ' 'the new constraint''s panel has Apply'
Ui-ExpectLog 'Added constraint Constraint1: align placed, moved O2' 'Apply added the align'
Ui-ExpectLog 'Applied Constraint (1)' 'the panel stayed open for the next one'
Ui-ExpectLog 'Added constraint Constraint2: insert placed, moved O3' 'OK added the insert'
Ui-ExpectLog 'Editing F5 with Constraint' 'a constraint of one row edited in the Constraint panel'
Ui-ExpectLog 'Edited F5' 'the edit'
Ui-ExpectLog 'TestDriver: done' 'Apply, OK, the edit and the undo steps'
Ui-StopApp
Write-Host 'PASS: Constraint adds one constraint at a time with Apply and OK'
