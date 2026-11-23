# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/BrowserPreferences.cpp app/browser/BrowserController.cpp app/view/ViewController.cpp app/view/PreferencesRestore.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/browser-control-places.mitcad-ui tools/ui-scripts/browser-control-places-docked.mitcad-ui tools/ui-scripts/browser-control-places.json
# The Browser rows' Activate radio button and Show eye places (mitcad#2223)
# in the build VM, as tools/ui-browser-control-places-test.sh on Linux: the
# shared steps tools/ui-scripts/browser-control-places.mitcad-ui (the
# Floating layout, Rows and Card) and browser-control-places-docked.mitcad-ui
# (the Docked layout) on the design that mitcad-cli makes
# (tools/ui-scripts/browser-control-places.json): with the defaults the
# radio buttons follow the names; on Right every radio button's and eye's
# centre has the same x at the visible right edge, the eye left of the radio
# button, and clicks activate and toggle; a long name is elided before them;
# Restore Defaults puts both back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-control-places-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-control-places'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'browser-control-places.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/browser-control-places.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# One of the shared scripts, the app started on the design in a layout, with
# the settings the runs share (the Floating run ends with Restore Defaults).
function Run-Script([string]$Name, [string]$Layout) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "browser control places driver timed out ($Name)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "browser control places driver failed ($Name, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the browser control places ($Name)"
  Ui-StopApp
}

Run-Script 'browser-control-places' 'floating'
Run-Script 'browser-control-places-docked' 'docked'
Write-Host 'PASS: the Browser rows put the Activate radio button and the Show eye where the preferences say'
