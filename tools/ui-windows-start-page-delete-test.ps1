# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowStart.cpp app/files/StartPage.cpp app/browser app/framework/TestDriver.cpp tools/cli
# Delete on the Start page (mitcad#1313) in the build VM, as
# tools/ui-start-page-delete-test.sh on Linux: the shared steps
# tools/ui-scripts/start-page-delete.mitcad-ui on a block that mitcad-cli
# makes (tools/ui-scripts/start-page-delete.json), in the Floating and the
# Docked layout: Sketch1 selected, the Start tab clears the selection, and
# Delete and Backspace on the page and on its New Design card delete
# nothing of the design behind it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-start-page-delete-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-start-page-delete'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'block.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/start-page-delete.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'block.mitcad was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/start-page-delete.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'

# One start of MitCad with the shared steps; the app ends with them.
function Run-Layout([string]$Name, [string]$Chrome) {
  Ui-StartApp "start-page-delete-$Name" @('--ui=qt', '--no-mcp', $Chrome, '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "${Name}: Delete on the Start page deletes nothing behind it"
  Ui-ExpectLog 'Timeline selected: nothing' "${Name}: the Start tab clears the selection"
}

Run-Layout 'floating' '--chrome=floating'
Run-Layout 'docked' '--chrome=docked'
Remove-Item Env:MITCAD_TEST_INPUT -ErrorAction SilentlyContinue
Ui-StopApp
Write-Host 'PASS: the Start page: Delete and Backspace delete nothing of the design behind it'
