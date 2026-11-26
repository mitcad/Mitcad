# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowToolbarPanel.cpp app/framework/ToolbarPanelReveal.cpp
# check-all sources: app/framework/ToolbarPanelReveal.hpp app/framework/FloatingLayout.cpp
# check-all sources: app/MainWindowStart.cpp app/MainWindow.cpp app/framework/Ribbon.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverPointer.cpp
# check-all sources: tools/ui-scripts/toolbar-panel-hide.mitcad-ui tools/ui-scripts/toolbar-panel-hide-restart.mitcad-ui
# View > Toolbar Panel (mitcad#2426, GitHub mitcad/Mitcad#571) on Windows, in
# the Floating and the Docked layout, each with a fresh profile and two starts
# running the shared input scripts as tools/ui-toolbar-panel-hide-test.sh runs
# them on Linux:
#   1. toolbar-panel-hide: Ctrl+F1 hides the panel (the view's room grows, the
#      way back said once), a rest at the view's top edge reveals it over the
#      view, a group's menu keeps it, a command from it runs and keeps it
#      (taking its room) while its command panel is open (mitcad#2472), it
#      goes once the command ended with the pointer away, the command by its
#      key keeps nothing, a drag across the edge shows nothing, View > Toolbar
#      Panel shows it, a new design and the Start page keep the hidden state.
#   2. toolbar-panel-hide-restart: hidden at the next start, revealed there,
#      Ctrl+F1 shows it and keeps it so.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-toolbar-panel-hide-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-toolbar-panel-hide'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
New-Item -ItemType Directory -Force (Split-Path $ini) | Out-Null
$env:MITCAD_TEST_STEP_MS = '100'

# Run-Panel layout script name: one start of MitCad in the layout running
# tools/ui-scripts/toolbar-panel-hide<script>.mitcad-ui; the app ends with it.
$script:Runs = 0
function Run-Panel([string]$Layout, [string]$Script, [string]$Name) {
  Write-Host "--- $Name"
  $script:Runs++
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/toolbar-panel-hide$Script.mitcad-ui"
  Ui-StartApp ("toolbar-panel-$($script:Runs)-$Layout") @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "$($Name): the toolbar panel driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$($Name): the toolbar panel driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' $Name
  Ui-LogLines | Where-Object { $_ -match '^Toolbar panel ' } | ForEach-Object { Write-Host "     $_" }
}

foreach ($layout in @('floating', 'docked')) {
  # A fresh profile for each layout (the way back is said once per profile).
  Set-Content -Encoding ascii $ini @('[shortcuts]')
  Run-Panel $layout '' "$($layout): hidden, revealed, a command from it, a new design, the Start page"
  if (-not (@(Get-Content $ini) -contains 'toolbarPanelHidden=true')) { Ui-Fail "$($layout): the hidden state is not kept in the settings" }
  Write-Host "ok   $($layout): the hidden state kept in the settings"
  Ui-StopApp
  Run-Panel $layout '-restart' "$($layout): hidden after the restart, shown again"
  if (-not (@(Get-Content $ini) -contains 'toolbarPanelHidden=false')) { Ui-Fail "$($layout): the shown state is not kept in the settings" }
  Write-Host "ok   $($layout): the shown state kept in the settings"
  Ui-StopApp
}
Write-Host 'PASS: toolbar panel hidden and revealed'
