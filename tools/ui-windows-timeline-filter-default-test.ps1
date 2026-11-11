# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/BrowserController.cpp app/framework/TestDriver.cpp tools/cli
# The timeline filter's default and saved choice (mitcad#1662) in the build
# VM, as tools/ui-timeline-filter-default-test.sh on Linux: the shared steps
# tools/ui-scripts/timeline-filter-default.mitcad-ui and
# timeline-filter-default-reopen.mitcad-ui on the design that mitcad-cli
# makes (tools/ui-scripts/timeline-filter-default.json), in the test's own
# settings (Ui-Init):
#   - The first start: Active Component with its subcomponents, every
#     feature listed with the root active; Base activated shows its and
#     Pin's features at once, the filter button highlighted; one named
#     component is not saved; All Components is.
#   - The window closed and started again: All Components.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timeline-filter-default-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timeline-filter-default'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'timeline-filter.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/timeline-filter-default.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

# Run-Script name: the app with the shared script ui-scripts/<name>.mitcad-ui
# on the design, until the script ends it.
function Run-Script([string]$Name) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  Ui-StartApp $Name @('--ui=qt', '--no-mcp', '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail "${Name}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${Name}: the driver failed (exit code $exitCode)"
  }
}

Write-Host '--- The first start: Active Component with its subcomponents'
Run-Script 'timeline-filter-default'
Ui-ExpectLog 'Timeline filter: Active Component (Root) with Subcomponents: 4 of 4 features' 'the timeline starts at Active Component: every feature with the root active'
Ui-ExpectLog 'Timeline filter: Active Component (Base) with Subcomponents: 3 of 4 features' 'Base activated: its features and its subcomponent''s'
Ui-ExpectLog 'Timeline filter: Base with Subcomponents: 3 of 4 features' 'one named component shown'
Ui-ExpectLog 'Timeline filter saved: all, subcomponents' 'All Components saved'
Ui-ExpectLog 'TestDriver: done' 'the first start''s script ran to its end'

Write-Host '--- The window started again: All Components'
Run-Script 'timeline-filter-default-reopen'
Ui-ExpectLog 'Timeline filter: all components' 'the next start takes the saved All Components'
Ui-ExpectLog 'Timeline filter saved: active, subcomponents' 'Active Component chosen again and saved'
Ui-ExpectLog 'TestDriver: done' 'the second start''s script ran to its end'
Ui-StopApp
Write-Host 'PASS: The timeline filter starts at Active Component and remembers the choice'
