# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandDockWidth.cpp app/framework/CommandDockWidth.hpp
# check-all sources: app/framework/UiSizes.cpp app/framework/UiSizes.hpp app/MainWindow.cpp
# check-all sources: app/sketch/SketchPalette.cpp app/sketch/SketchPalette.hpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/docked-command-width.mitcad-ui
# check-all sources: tools/ui-scripts/docked-command-width-restart.mitcad-ui
# The Docked layout's Command panel at the largest Text size and Icon size
# (mitcad#1825, GitHub mitcad/Mitcad#336) in the build VM, as
# tools/ui-docked-command-width-test.sh on Linux: the shared steps
# tools/ui-scripts/docked-command-width.mitcad-ui and
# docked-command-width-restart.mitcad-ui, MitCad started Docked with
# appearance/textSize and appearance/iconSize 200. The panel is at least
# 600 px wide and every row of the Sketch Palette lies whole within it; its
# edge dragged wider is remembered (commandPanel/dockWidth) and restored
# after a restart, the rows whole at each width.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-docked-command-width-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-docked-command-width'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
New-Item -ItemType Directory -Force (Split-Path $ini) | Out-Null
$env:MITCAD_TEST_STEP_MS = '100'
# The sizes alone: no scale factor of the environment's.
Remove-Item Env:QT_SCALE_FACTOR -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_USER_QT_SCALE_FACTOR -ErrorAction SilentlyContinue
Set-Content -Encoding ascii $ini @('[appearance]', 'textSize=200', 'iconSize=200')

# Run-Script name script: one Docked start with the input script; the app
# ends with the script's steps.
function Run-Script([string]$Name, [string]$Script) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  Write-Host "--- $Name"
  Ui-StartApp ($Script -replace '\.mitcad-ui$', '') @('--ui=qt', '--no-mcp', '--chrome=docked')
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "$($Name): the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "$($Name): the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' "$($Name): the script's steps"
  Ui-LogLines | Where-Object { $_ -match '^(Command panel|Command edge at |Palette rows cut: |Palette buttons clipped: )' } |
    Select-Object -Last 8 | ForEach-Object { Write-Host "     $_" }
}

# The first and the last "Command panel width <w>" of the log.
function Panel-Width([switch]$Last) {
  $widths = @(Ui-LogLines | ForEach-Object {
      $match = [regex]::Match($_, '^Command panel width (\d+) ')
      if ($match.Success) { [int]$match.Groups[1].Value }
    })
  if ($widths.Count -eq 0) { return 0 }
  if ($Last) { return $widths[-1] }
  return $widths[0]
}

Run-Script 'Docked, text and icons 200%' 'docked-command-width.mitcad-ui'
$first = Panel-Width
if ($first -lt 600) { Ui-Fail "the panel is $first px wide at 200 %, not at least 600" }
Write-Host "ok   the panel opened $first px wide (at least 600)"
$remembered = @(Ui-LogLines | ForEach-Object {
    $match = [regex]::Match($_, '^Command panel: dock width (\d+) remembered$')
    if ($match.Success) { [int]$match.Groups[1].Value }
  })
if ($remembered.Count -eq 0) { Ui-Fail 'the dragged width was not remembered' }
$width = $remembered[-1]
if ($width -lt $first + 100) { Ui-Fail "the dragged panel is $width px wide (was $first)" }
if (-not (Select-String -Quiet -Path $ini -Pattern "^dockWidth=$width$")) { Ui-Fail "the width $width is not kept" }
Write-Host "ok   the dragged panel is $width px wide, remembered"
Ui-StopApp

# Without the docks' kept arrangement (mitcad#2070), which would give the
# panel its width at quit instead: the remembered width's own restore, as at
# a start with nothing kept (tools/ui-windows-dock-state-test.ps1 tests the
# other).
$section = ''
$kept = @()
foreach ($line in @(Get-Content $ini)) {
  if ($line -match '^\[(.*)\]$') { $section = $Matches[1] }
  if ($section -ne 'dockState') { $kept += $line }
}
Set-Content -Encoding ascii $ini $kept
$env:MITCAD_TEST_COMMAND_WIDTH = "$width"
Run-Script 'Docked, restarted' 'docked-command-width-restart.mitcad-ui'
$restored = Panel-Width -Last
if ($restored -lt $width - 10 -or $restored -gt $width + 10) {
  Ui-Fail "the panel is $restored px wide after the restart, not $width"
}
Write-Host "ok   the panel is $restored px wide again ($width remembered)"
Ui-StopApp
Write-Host 'PASS: the Docked Command panel at Text size and Icon size 200 %'
