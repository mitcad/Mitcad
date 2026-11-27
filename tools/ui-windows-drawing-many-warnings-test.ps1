# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/CommandDockWidth.cpp app/framework/HangWatchdog.cpp tools/cli
# check-all sources: app/framework/TestDriver.cpp app/browser/BrowserController.cpp
# check-all sources: tools/ui-scripts/drawing-many-warnings.mitcad-ui tools/ui-scripts/drawing-many-warnings.json
# A base view with many warnings (mitcad#2507, follow-up of mitcad#2476;
# GitHub mitcad/Mitcad#694: the window stopped answering once a view that
# kept 141 bodies that were gone was selected) in the build VM, as
# tools/ui-drawing-many-warnings-test.sh on Linux: the shared steps
# tools/ui-scripts/drawing-many-warnings.mitcad-ui on the design that
# mitcad-cli makes (tools/ui-scripts/drawing-many-warnings.json: Plate placed
# 60 times, a drawing whose base view at 1:20 draws them all, 49 of the
# placements then deleted), Docked, the hang watchdog on at 15 s: the view
# selected says its warning once with its count and the drawing's area keeps
# its size, Close Drawing shows the 3D view again, the drawing opened again
# and its view selected, with no "UI not responding" line all along.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-drawing-many-warnings-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-drawing-many-warnings'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'plates.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/drawing-many-warnings.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/drawing-many-warnings.mitcad-ui'
# The hang watchdog on, with room for the slow steps of the VM's renderer:
# the window that stopped answering did so for minutes.
$env:MITCAD_HANG_WATCHDOG_MS = '15000'
# Docked: Ui-StartApp's default (--chrome=docked), as on Linux.
Ui-StartApp 'drawing-many-warnings' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'many warnings driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "many warnings driver failed (exit code $exitCode)"
}
Remove-Item Env:MITCAD_HANG_WATCHDOG_MS -ErrorAction SilentlyContinue
if ((Ui-CountLog '\[hang watchdog\]') -gt 0) { Ui-Fail 'the window stopped answering (a hang watchdog line)' }
Write-Host 'ok   no line of the hang watchdog'
Ui-ExpectLog 'Drawing view 1 panel warnings (49 of them): an occurrence is gone (49 times)' 'the warning said once, with how many times it came'
Ui-ExpectLog 'Drawing closed' 'Close Drawing showed the 3D view again'
Ui-ExpectLog 'TestDriver: done' 'the drawing opened again, its view selected, the window answering'
Ui-StopApp
Write-Host 'PASS: drawing view with many warnings'
