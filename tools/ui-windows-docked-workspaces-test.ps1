# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/WindowStyle.cpp app/DocumentFrame.cpp app/projects/MainWindowProjectsPanel.cpp app/MainWindow.cpp app/MainWindowDocks.cpp app/MainWindowRender.cpp app/MainWindowStart.cpp app/drawings/MainWindowDrawings.cpp app/framework/QuickAccessGroup.cpp app/framework/TitleBar.cpp app/framework/BoostButton.cpp app/agent/MainWindowChat.cpp app/framework/TestDriver.cpp app/framework/TestDriverDocks.cpp app/render/RenderWorkspace.cpp tools/cli tools/ui-scripts/docked-workspaces.mitcad-ui tools/ui-scripts/workspaces.json
# The Docked layout of Windows and Linux (mitcad#2082, GitHub
# mitcad/Mitcad#476) in the build VM, as tools/ui-docked-workspaces-test.sh
# on Linux: the shared steps tools/ui-scripts/docked-workspaces.mitcad-ui on
# a box with one drawing (tools/ui-scripts/workspaces.json): the Floating
# layout's top bar at the window's top, no menu bar row, the toolbar panel
# under it; the Projects panel hidden at the start although the old Docked
# choice says shown; Make's three tabs alone; Render's tab, docks and
# rendered view, Design bringing the tabs, the docks and the view mode
# back; none of Render's docks in the panels' menu outside it; a drawing
# hiding the model's docks (the Browser stays) and Close Drawing showing
# exactly those again; Home showing the Start page without the panel, the
# design's tab leading back; the selector locked while sketching.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-docked-workspaces-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-docked-workspaces'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
# The Docked layout's old choice of the Projects panel (shown): not read by
# the top bar's layout.
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true' }
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'workspaces.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/workspaces.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/docked-workspaces.mitcad-ui'
Ui-StartApp 'docked-workspaces' @('--ui=qt', '--no-mcp', '--chrome=docked', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'docked workspaces driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "docked workspaces driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'Menu bar hidden: Docked' 'no menu bar row'
Ui-ExpectLog 'Projects panel at the start: hidden (no choice yet in this layout)' 'the Projects panel hidden at the start'
Ui-ExpectLog 'Docked top bar: window 1280x800' "the top bar's parts in the Floating order"
$standIn = @(Get-Content $script:UiLog | Where-Object { $_ -match '^(Quick access group: flat|Boost button row: |Ribbon tab row end: )' })
if ($standIn.Count -gt 0) { Ui-Fail "a stand-in of the old Docked toolbar: $($standIn[0])" }
Write-Host 'ok   no quick-access group, Boost or search field in a tab row or menu bar'
Ui-ExpectLog 'Ribbon workspace: MAKE (tabs MAKE_PRINT, MAKE_LASER, MAKE_CNC)' 'Make: its three tabs alone'
Ui-ExpectLog 'Ribbon workspace: RENDER (tabs RENDER)' 'Render: its tab alone'
Ui-ExpectLog 'Workspace RENDER left: the rendered view off' 'Design: the view mode of before'
Ui-ExpectLog 'Workspace DRAWING: opened D1' "Drawing: the design's drawing opened"
Ui-ExpectLog 'Drawing docks hidden: commandDock, projectsDock' "a drawing hides the model's docks shown"
Ui-ExpectLog 'Drawing docks shown again: commandDock, projectsDock' 'Close Drawing shows them again'
Ui-ExpectLog 'Docked toolbar panel hidden (Start)' 'Home: the Start page without the toolbar panel'
Ui-ExpectLog 'Start page left by the design' "the design's tab leads back"
Ui-ExpectLog 'Docked toolbar panel shown' 'the toolbar panel back'
Ui-ExpectLog 'TestDriver: done' 'the Docked workspaces'
Get-Content $script:UiLog | Where-Object { $_ -like 'Docked top bar: *' -or $_ -like 'Title row tabs: *' -or $_ -like 'Ribbon workspace: *' -or $_ -like 'Drawing docks *' -or $_ -like '*TestDriver: dock *' } | ForEach-Object { Write-Host "     $_" }
Ui-StopApp
Write-Host 'PASS: the Docked top bar, its workspace selector, its Render docks and the docks a drawing hides'
