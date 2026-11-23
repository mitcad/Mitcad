# SPDX-License-Identifier: MIT
# check-all sources: app/browser/FavoriteParametersCard.cpp app/browser/FavoriteParameterRows.cpp
# check-all sources: app/browser/MainWindowFavoriteParameters.cpp tools/ui-scripts/favorite-parameters.mitcad-ui
# Favorite Parameters (mitcad#2203, GitHub mitcad/Mitcad#547) in the build
# VM, as tools/ui-favorite-parameters-test.sh on Linux: the shared steps
# tools/ui-scripts/favorite-parameters.mitcad-ui on the design mitcad-cli
# makes in the work folder (tools/ui-scripts/favorite-parameters.json), in
# the Floating layout and then in the Docked one: the empty card, two
# favorites starred in the Parameters window, a typed value, the slider,
# a refused value, undo, a favorite unstarred, the toggle. After each a
# second start finds the card shown as chosen and its own close button
# hides it (favorite-parameters-close-floating.mitcad-ui and
# -close-docked.mitcad-ui). Pictures of the card, light and dark, stay in
# the work folder.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-favorite-parameters-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-favorite-parameters'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'favorite-parameters.mitcad'
$output = & $Cli run (Join-Path $PSScriptRoot 'ui-scripts/favorite-parameters.json') --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: the design was not made: $($output -join ' / ')" }
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'

function Run-Script([string]$Script, [string]$Layout, [string]$What) {
  $env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot "ui-scripts/$Script"
  $env:FAVORITES_LAYOUT = $Layout
  Ui-StartApp "favorite-parameters-$Layout-$($Script.Replace('.mitcad-ui', ''))" @('--ui=qt', '--no-mcp', "--chrome=$Layout", '--open', $design)
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail "${What}: the driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${What}: the driver failed (exit code $exitCode)"
  }
  Ui-ExpectLog 'TestDriver: done' $What
}

foreach ($layout in @('floating', 'docked')) {
  Run-Script 'favorite-parameters.mitcad-ui' $layout "Favorite Parameters ($layout)"
  Ui-ExpectLog 'Favorite parameter width set: 900 mm = 900 mm' "${layout}: a typed value, the unit added"
  Ui-ExpectLog 'Favorite parameter width refused: 2600: ' "${layout}: a value above the strict maximum refused"
  Ui-ExpectLog 'Favorite parameters card: none' "${layout}: the empty card"
  foreach ($picture in @('empty', 'light', 'dark', 'drag')) {
    $path = Join-Path $work "favorite-parameters-$picture-$layout.png"
    if (-not (Test-Path $path)) { Ui-Fail "no picture $picture ($layout)" }
  }
  Run-Script "favorite-parameters-close-$layout.mitcad-ui" $layout "Favorite Parameters closed ($layout)"
  Ui-ExpectLog 'Favorite parameters panel at the start: shown (as chosen)' "${layout}: shown as chosen"
  Ui-ExpectLog 'Favorite parameters: closed by its button' "${layout}: its own close button hides it"
}
Ui-StopApp
Write-Host 'PASS: Favorite Parameters in the Floating and Docked layouts'
