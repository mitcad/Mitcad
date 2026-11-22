# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/DocumentSnapshot.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/new-component-fillets.mitcad-ui tools/ui-scripts/new-component-fillets.json
# An extrude edited from Join to New Component takes its fillet along
# (mitcad#2185) in the build VM, as tools/ui-new-component-fillets-test.sh
# on Linux: the shared steps tools/ui-scripts/new-component-fillets.mitcad-ui
# on the design that mitcad-cli makes
# (tools/ui-scripts/new-component-fillets.json): no feature fails, and the
# timeline marks the extrude and the fillet in the new component's colour
# at once; undo and the edit back to Join bring the component's colour back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-new-component-fillets-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-new-component-fillets'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'new-component-fillets.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/new-component-fillets.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/new-component-fillets.mitcad-ui'
Ui-StartApp 'new-component-fillets' @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail 'new component fillets driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "new component fillets driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Timeline feature colors: ' 'the timeline logs its colour marks'
Ui-ExpectLog 'TestDriver: done' 'the fillet followed the new component'
Ui-StopApp
Write-Host 'PASS: an extrude edited to New Component takes its fillet along'
