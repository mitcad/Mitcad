# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/TimelineHistoryList.cpp app/browser/MainWindowTimelineList.cpp app/browser/BrowserRows.hpp app/browser/BrowserPanel.cpp app/browser/BrowserPreferences.cpp app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp app/view/ViewController.cpp app/MainWindowDocks.cpp app/framework/GlassCard.cpp app/framework/TestDriver.cpp tools/cli tools/ui-scripts/timeline-history-list.mitcad-ui tools/ui-scripts/timeline-history-list-docked.mitcad-ui tools/ui-scripts/timeline-history-list.json
# The timeline's History list (mitcad#2427, GitHub mitcad/Mitcad#581) in the
# build VM, as tools/ui-timeline-history-list-test.sh on Linux: the shared
# steps tools/ui-scripts/timeline-history-list.mitcad-ui (the Floating
# layout) and timeline-history-list-docked.mitcad-ui (the Docked layout) on
# the design that mitcad-cli makes (tools/ui-scripts/timeline-history-list.json):
# Preferences > Display > Timeline: History list moves the timeline under
# the Browser, its rows name the items in timeline order with the group as a
# folder row; the marker dragged between two rows rolls the model back; the
# group's arrow folds it and opens it; a sketch's eye hides it; the context
# menu's Rename renames in the row; Strip brings the strip back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timeline-history-list-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timeline-history-list'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'timeline-history-list.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/timeline-history-list.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# One of the shared scripts, the app started on the design in a layout, with
# the settings the runs share (the Floating run ends with the strip again).
function Run-Script([string]$Name, [string]$Layout) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "timeline history list driver timed out ($Name)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "timeline history list driver failed ($Name, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the timeline history list ($Name)"
  Ui-StopApp
}

Run-Script 'timeline-history-list' 'floating'
Run-Script 'timeline-history-list-docked' 'docked'
Write-Host 'PASS: the History list shows the timeline under the Browser in both layouts, and the strip comes back'
