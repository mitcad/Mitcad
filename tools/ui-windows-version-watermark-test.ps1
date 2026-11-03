# SPDX-License-Identifier: MIT
# check-all sources: app/view/VersionWatermark.cpp app/view/VersionWatermark.hpp app/MainWindow.cpp
# check-all sources: app/view/ViewController.cpp app/framework/FloatingLayout.cpp app/OcctViewer.cpp
# Input-driven Windows UI test of MitCad's version in the main window
# (mitcad#1209) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), on the demo block, in the Floating and the Docked
# layout, as tools/ui-version-watermark-test.sh does on Linux:
#   1. The watermark's text is "MitCad <version>" (the build's version,
#      -Version); the window title ends in " - MitCad <version>".
#   2. The watermark sits in the view's lower right: at the view's right
#      edge, in the Docked layout at the view's bottom, in the Floating
#      layout just above the timeline's card (and the message band's row).
#   3. A click at its place reaches the view: the selection (Body1, from
#      the browser) is cleared.
#   4. Preferences > General > Show the version in the view (Alt+E) off
#      hides it at once, the title keeps the version; on shows it again.
# The view's picture without the watermark is checked on Linux.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-version-watermark-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir] [-Version x.y.z]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-version-watermark'),
  [string]$QtBin = '',
  [string]$Version = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.preferences.general' = 'F8' } -StepPause 0

# Placed: the watermark's last place, @(bx, by, bw, bh, vx, vy, vw, vh): its
# box and the view's in the window's coordinates.
function Placed {
  $line = "$(Ui-LastLineWith "Version watermark 'MitCad $Version' at ")"
  $match = [regex]::Match($line, ': box (-?\d+),(-?\d+) (\d+)x(\d+), view (-?\d+),(-?\d+) (\d+)x(\d+)$')
  if (-not $match.Success) { Ui-Fail "no place of the watermark logged: '$line'" }
  return @(1..8 | ForEach-Object { [int]$match.Groups[$_].Value })
}

# Expect-Title layout: the window title ends in the version.
function Expect-Title([string]$Layout) {
  $title = [UiInput]::Title($script:UiWindow)
  if (-not $title.EndsWith(" - MitCad $Version")) {
    Ui-Fail "${Layout}: the window title '$title' does not end in ' - MitCad $Version'"
  }
  Write-Host "ok   ${Layout}: the window title ends in the version: $title"
}

# Toggle-Preference layout on|off: Preferences on its General page, Show
# the version in the view (Alt+E), OK.
function Toggle-Preference([string]$Layout, [string]$State) {
  Ui-Mark
  Ui-Step 'Preferences: General (F8)'      { Ui-Key 'F8' }
  Ui-ExpectNew 'Preferences opened: General, ' "${Layout}: Preferences on its General page"
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'Show the version in the view (Alt+E)' { Ui-Key 'alt+e' }
  Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Preferences: show the version in the view $State" "${Layout}: the preference saved ($State)"
}

function Round([string]$Layout, [string]$Chrome) {
  Write-Host "--- The $Layout layout"
  Ui-StartApp "version-watermark-$Chrome" @('--demo', "--chrome=$Chrome")
  if (-not $script:Version) {
    Ui-ExpectLog "Version watermark 'MitCad " "${Layout}: the watermark is logged"
    $script:Version = Ui-LastMatch "Version watermark 'MitCad (\S+)' at "
    if (-not $script:Version) { Ui-Fail "${Layout}: no watermark logged" }
    Write-Host "note no -Version given: the watermark's own, $($script:Version)"
  }
  Ui-ExpectLog "Version watermark 'MitCad $Version' at " "${Layout}: the watermark shows 'MitCad $Version'"
  Expect-Title $Layout
  Ui-Sync
  $bx, $by, $bw, $bh, $vx, $vy, $vw, $vh = Placed
  if ($bx -lt $vx -or $by -lt $vy -or $bx + $bw -gt $vx + $vw -or $by + $bh -gt $vy + $vh) {
    Ui-Fail "${Layout}: the watermark ($bx,$by ${bw}x$bh) is not inside the view ($vx,$vy ${vw}x$vh)"
  }
  if ($vx + $vw - $bx - $bw -gt 24) {
    Ui-Fail "${Layout}: the watermark ($bx,$by ${bw}x$bh) is not at the view's right edge ($vx,$vy ${vw}x$vh)"
  }
  if ($Chrome -eq 'docked') {
    if ($vy + $vh - $by - $bh -gt 24) {
      Ui-Fail "${Layout}: the watermark ($bx,$by ${bw}x$bh) is not at the view's bottom ($vx,$vy ${vw}x$vh)"
    }
    Write-Host "ok   ${Layout}: in the view's lower right, $bx,$by ${bw}x$bh"
  } else {
    $cards = "$(Ui-LastLineWith 'Floating cards: ')"
    $timeline = [regex]::Match($cards, '; timelineDock (\d+),(\d+) (\d+)x(\d+)')
    if (-not $timeline.Success) { Ui-Fail "${Layout}: no timeline card logged: '$cards'" }
    $floor = [int]$timeline.Groups[2].Value
    $band = [regex]::Match($cards, '; messageBandRow \d+,(\d+) ')
    if ($band.Success -and [int]$band.Groups[1].Value -lt $floor) { $floor = [int]$band.Groups[1].Value }
    if ($by + $bh -gt $floor -or $floor - $by - $bh -gt 40) {
      Ui-Fail "${Layout}: the watermark ($bx,$by ${bw}x$bh) is not just above the timeline or the message band (y $floor)"
    }
    Write-Host "ok   ${Layout}: just above the timeline, $bx,$by ${bw}x$bh (y $floor below it)"
  }

  Write-Host '--- A click at its place reaches the view'
  Ui-Mark
  Ui-Step 'select Body1 in the browser'   { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
  Ui-ExpectNew 'Selected: 1 body' "${Layout}: Body1 selected"
  Ui-Mark
  Ui-Step 'click the watermark'           { Ui-ClickLogged "Version watermark 'MitCad $Version'" }
  Ui-ExpectNew 'Selected: nothing' "${Layout}: the click on the watermark is the view's (the selection cleared)"

  Write-Host '--- The preference'
  Toggle-Preference $Layout 'off'
  Ui-ExpectNew 'Version watermark hidden: preference' "${Layout}: the watermark hidden at once"
  Expect-Title $Layout
  Toggle-Preference $Layout 'on'
  Ui-ExpectNew "Version watermark 'MitCad $Version' at " "${Layout}: the watermark shown again"
  Ui-StopApp
}

Round 'Floating' 'floating'
Round 'Docked' 'docked'
Ui-Finish 'UI Windows version watermark test'
