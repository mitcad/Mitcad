# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandLayout.cpp app/resources/layout/default.json app/view/ViewController.cpp app/MainWindow.cpp app/framework/TestDriver.cpp tools/ui-scripts/command-layout.mitcad-ui tools/ui-scripts/command-layout-restart.mitcad-ui
# The command layout's user layer (mitcad#1951, mitcad#1991) in the build
# VM, as tools/ui-command-layout-test.sh on Linux: the shared steps
# tools/ui-scripts/command-layout.mitcad-ui, then after a restart
# command-layout-restart.mitcad-ui, in the Floating and the Docked layout.
# Sweep moved from Create to Modify, then shown in both groups; Revolve's
# button removed by its right-click and Inspect moved after Modify; after
# the restart both kept, Show on Toolbar brings the button back, Remove
# from Toolbar takes it away again, and Preferences > Restore Default Layout
# puts everything back.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-command-layout-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-command-layout'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'

# One of the shared scripts in one layout, on the settings the runs share.
function Run-Script([string]$Name, [string]$Layout) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Name.mitcad-ui"
  Ui-StartApp "$Name-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail "command layout driver timed out ($Name, $Layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "command layout driver failed ($Name, $Layout, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the command layout ($Name, $Layout)"
}

# Both scripts in one layout, the second after a restart.
function Run-Layout([string]$Layout) {
  Run-Script 'command-layout' $Layout
  Ui-ExpectLog 'Layout: moved solid.sweep from SOLID/CREATE to SOLID/MODIFY' "Sweep moved ($Layout)"
  Ui-ExpectLog 'Layout: removed the button of solid.revolve from SOLID/CREATE' "Revolve's button removed ($Layout)"
  Ui-ExpectLog 'Layout: moved the group SOLID/INSPECT: CREATE, MODIFY, INSPECT, ASSEMBLE' "Inspect moved after Modify ($Layout)"
  Ui-StopApp
  Run-Script 'command-layout-restart' $Layout
  Ui-ExpectLog 'Ribbon groups of SOLID: CREATE, MODIFY, INSPECT, ASSEMBLE' "the order kept after a restart ($Layout)"
  Ui-ExpectLog 'Layout: showed the button of solid.revolve in SOLID/CREATE again' "Show on Toolbar ($Layout)"
  Ui-ExpectLog 'Preferences: layout restored to the default' "the default restored ($Layout)"
  Ui-StopApp
}

Run-Layout 'floating'
Run-Layout 'docked'
Write-Host 'PASS: a command moved and shown in two groups, a default button removed and a group moved through the user layout, kept after a restart, and the default restored'
