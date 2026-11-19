# SPDX-License-Identifier: MIT
# check-all sources: app/framework/FloatingLayout.cpp app/framework/FloatingLayout.hpp
# check-all sources: app/framework/UiSizes.cpp app/framework/UiSizes.hpp app/MainWindow.cpp
# check-all sources: app/sketch/SketchPalette.cpp app/sketch/SketchPalette.hpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/floating-command-width.mitcad-ui
# The Floating layout's Command card at the largest Text size and Icon size
# (mitcad#1846, follow-up of mitcad#1825) in the build VM, as
# tools/ui-floating-command-width-test.sh on Linux: the shared steps
# tools/ui-scripts/floating-command-width.mitcad-ui, MitCad started Floating
# with appearance/textSize and appearance/iconSize 200. The card is at least
# 600 px wide with Box's panel and in a sketch, every row of the Sketch
# Palette lies whole within it and no button is clipped.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-floating-command-width-test.ps1 -App build\dev\mitcad.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-floating-command-width'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
New-Item -ItemType Directory -Force (Split-Path $ini) | Out-Null
$env:MITCAD_TEST_STEP_MS = '100'
# The sizes alone: no scale factor of the environment's.
Remove-Item Env:QT_SCALE_FACTOR -ErrorAction SilentlyContinue
Remove-Item Env:MITCAD_USER_QT_SCALE_FACTOR -ErrorAction SilentlyContinue
# Icons only: the toolbar panel without its labels (mitcad#1990), as the
# script's steps were laid out.
Set-Content -Encoding ascii $ini @('[appearance]', 'textSize=200', 'iconSize=200', 'ribbonLabels=icons')

$name = 'Floating, text and icons 200%'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/floating-command-width.mitcad-ui'
Write-Host "--- $name"
Ui-StartApp 'floating-command-width' @('--ui=qt', '--no-mcp', '--chrome=floating')
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(120000)) { Ui-Fail "$($name): the driver timed out" }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "$($name): the driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "$($name): the script's steps"
Ui-LogLines | Where-Object { $_ -match '^(Palette rows cut: |Palette buttons clipped: |Palette Constraints: )' } |
  Select-Object -Last 3 | ForEach-Object { Write-Host "     $_" }

# The command card's width in the last "Floating cards" line that has it.
$widths = @(Ui-LogLines | ForEach-Object {
    if ($_ -match '^Floating cards: ') {
      $match = [regex]::Match($_, '; commandDock -?\d+,-?\d+ (\d+)x')
      if ($match.Success) { [int]$match.Groups[1].Value }
    }
  })
if ($widths.Count -eq 0) { Ui-Fail 'no command card in the Floating cards lines' }
$width = $widths[-1]
if ($width -lt 600) { Ui-Fail "the command card is $width px wide at 200 %, not at least 600" }
Write-Host "ok   the command card is $width px wide (at least 600)"
if (-not (Ui-LogLines | Where-Object { $_ -match '^Palette rows cut: none \(' })) {
  Ui-Fail 'a row of the Sketch Palette is cut in the card'
}
Write-Host "ok   the Sketch Palette's rows are whole in the card"
if (-not (Ui-LogLines | Where-Object { $_ -match '^Palette buttons clipped: none ' })) {
  Ui-Fail 'a button of the Sketch Palette is clipped'
}
Write-Host 'ok   no button of the Sketch Palette is clipped'
Ui-StopApp
Write-Host 'PASS: the Floating Command card at Text size and Icon size 200 %'
