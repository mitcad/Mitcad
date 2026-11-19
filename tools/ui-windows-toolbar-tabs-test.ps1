# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandLayout.cpp app/framework/TitleBar.cpp app/resources/layout/default.json app/framework/TestDriver.cpp tools/ui-scripts/toolbar-tabs.mitcad-ui
# The tabs' menu (mitcad#1975) in the build VM, as
# tools/ui-toolbar-tabs-test.sh on Linux: the shared steps
# tools/ui-scripts/toolbar-tabs.mitcad-ui in the Floating and the Docked
# layout. Surface hidden from a right-click on a tab and shown again from
# the + after the last tab, Customize Toolbar... giving Sweep a button,
# Reset Toolbar from a command row's menu taking it away.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-toolbar-tabs-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-toolbar-tabs'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/toolbar-tabs.mitcad-ui'

# The shared script in one layout.
function Run-Layout([string]$Layout) {
  Ui-StartApp "toolbar-tabs-$Layout" @('--ui=qt', '--no-mcp', "--chrome=$Layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(150000)) { Ui-Fail "toolbar tabs driver timed out ($Layout)" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "toolbar tabs driver failed ($Layout, exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "the tabs menu ($Layout)"
  Ui-ExpectLog 'Ribbon tab SURFACE hidden' "Surface hidden ($Layout)"
  Ui-ExpectLog 'Ribbon tab SURFACE shown' "Surface shown again ($Layout)"
  Ui-ExpectLog 'Customize Toolbar SOLID: solid.sweep on' "Sweep's button from Customize Toolbar ($Layout)"
  Ui-ExpectLog 'Toolbar reset: SOLID' "the Solid tab reset ($Layout)"
  Ui-StopApp
}

Run-Layout 'floating'
Run-Layout 'docked'
Write-Host 'PASS: tabs hidden and shown from the tabs menu, Customize Toolbar and Reset Toolbar'
