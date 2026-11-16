# SPDX-License-Identifier: MIT
# check-all sources: app/update app/MainWindow.cpp app/WindowManager.cpp app/framework/TestDriver.cpp tools/cli
# One update for the whole application (mitcad#1890) in the build VM, as
# tools/ui-update-one-row-test.sh on Linux: the shared steps
# tools/ui-scripts/update-one-row.mitcad-ui with the update tests' fake
# offer (the test driver's step update-fake-offer, in any build: no server,
# no request), on two cubes that mitcad-cli makes
# (tools/ui-scripts/panel-message-top.json): Install accepted in the first
# design's window, a second design opened shows the download in its row at
# once ("Update row: in 2 window(s)"), the first design closed leaves the
# row in the second, where the download goes on ("Update download: N%")
# and reaches "downloaded"; then Exit during a download asks, and Keep
# Downloading keeps MitCad open until it is downloaded.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-update-one-row-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-update-one-row'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
foreach ($name in @('first', 'second')) {
  & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/panel-message-top.json') --save (Join-Path $work "$name.mitcad")
  if ($LASTEXITCODE -ne 0) { Ui-Fail "the design $name was not made" }
}
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/update-one-row.mitcad-ui'

# Checks off (no -Updates): no automatic check and no request; the fake
# offer answers Check for Updates all the same.
Ui-StartApp 'update-one-row' @('--ui=qt', '--no-mcp', '--open', (Join-Path $work 'first.mitcad'))
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'the update driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "the update driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the shared steps ran to their end'
Ui-ExpectLog 'Update row: in 2 window(s)' "the second design's window shows the update's row"
# After the first window closed: the download went on in the second.
$all = @(Ui-LogLines)
$from = -1
$two = $false
for ($i = 0; $i -lt $all.Count; $i++) {
  if ($all[$i] -eq 'Update row: in 2 window(s)') { $two = $true }
  if ($two -and $all[$i] -eq 'Update row: in 1 window(s)') { $from = $i; break }
}
if ($from -lt 0) { Ui-Fail 'the first window did not close' }
$after = $all[$from..($all.Count - 1)]
if (-not ($after | Where-Object { $_ -match '^Update download: [0-9]+%$' })) {
  Ui-Fail 'no progress of the download after the first window closed'
}
if (-not ($after | Where-Object { $_.Contains('Update notice (message): MitCad 999.0.0 is downloaded.') })) {
  Ui-Fail "the second window's row did not reach downloaded"
}
Write-Host 'ok   the download went on in the second window and reached downloaded'
if ($all | Where-Object { $_ -match '^Update: cancelled' }) { Ui-Fail 'the download was cancelled' }
Ui-ExpectLog 'Update question: MitCad 999.0.0 is still downloading' 'Exit during the download asks'
Ui-ExpectLog 'Update: the download goes on, MitCad stays open' 'Keep Downloading keeps MitCad open'
Ui-StopApp
Write-Host 'PASS: one update for the application'
