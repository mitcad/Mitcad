# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserBackground.cpp app/browser/BrowserPanel.cpp app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp app/MainWindow.cpp app/view/ViewController.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/browser-widths.mitcad-ui tools/ui-scripts/browser-widths-restart.mitcad-ui tools/ui-scripts/browser-widths.json
# The Browser's width in the Floating layout (mitcad#1932) in the build VM,
# as tools/ui-browser-widths-test.sh on Linux: the shared steps
# tools/ui-scripts/browser-widths.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/browser-widths.json): Rows at the start without a
# saved choice (mitcad#2144); with Rows a long component
# name widens the rows and the card, a longer one stops at a third of the
# view and is elided, a limit changed in Preferences takes effect, the card
# shrinks back and a click beside the rows reaches the view; with Card the
# dragged width grip widens the card; then
# tools/ui-scripts/browser-widths-restart.mitcad-ui after a restart: the
# width comes back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-widths-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-widths'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'browser-widths.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-widths.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# One of the shared scripts, the app started on the design in the Floating
# layout, with the settings the runs share.
function Run-Script([string]$Name) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', '--chrome=floating', '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "browser widths driver timed out ($Name)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "browser widths driver failed ($Name, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the browser widths ($Name)"
  Ui-StopApp
}

Run-Script 'browser-widths'
Ui-ExpectLog 'Browser card width 380 remembered' 'the dragged Card width is remembered'
Run-Script 'browser-widths-restart'
Ui-ExpectLog 'Browser card width 380 (remembered)' 'the Card width comes back after a restart'
Write-Host 'PASS: the Browser widens to its rows, and its card keeps the width dragged'
