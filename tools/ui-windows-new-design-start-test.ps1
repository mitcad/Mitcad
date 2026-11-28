# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/DocumentSession.hpp core/model/src/document/display.rs app/view/DefaultView.hpp
# Input-driven Windows UI test of how a new design opens (mitcad#1683,
# mitcad#1809), in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted); the Linux test is tools/ui-new-design-start-test.sh:
#   1. The untitled design at the start shows its origin (the design's
#      origin shown in the view, the Origin folder's light bulb on) with no
#      undo step (the title has no "*"), and looks from the front right and
#      above along the default view (mitcad#1698, a few degrees off the
#      isometric diagonal), Z up, not at Top (mitcad#1809), with the origin
#      within three pixels of the view's free area's centre ("View: new
#      design looks along ..."), the camera at rest along it.
#   2. Create Sketch does not turn that view: a click on YZ, XZ and XY each
#      starts a sketch on its plane, and each sketch, finished, returns to
#      the default view. A view turned to Front (on F8 here) before the
#      first sketch keeps Front after Finish Sketch.
#   3. A design read from a file keeps the origin it saved (hidden, light
#      bulb off; shown, light bulb on), is seen from the default view, and
#      no new design's framing is logged for it.
#   4. New Design (Ctrl+N, in the current window) after an opened design
#      turned to Top (on F12 here) opens from the default view again, with
#      the origin shown.
# The other UI tests keep the start they were written for
# (MITCAD_NEW_DESIGN_VIEW=0, Ui-StartApp: the exact isometric view, the
# origin hidden); this one sets it to 1.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-new-design-start-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-new-design-start'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# New replaces the window's design (no second window to follow).
# View > Front and View > Top on keys of their own.
Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'view.front' = 'F8'; 'view.top' = 'F12';
  'documents/openInCurrentWindow' = 'true' } -StepPause 0
$env:MITCAD_NEW_DESIGN_VIEW = '1'

# The default view's direction (view/DefaultView.hpp: azimuth 41 degrees,
# elevation 30) and its up (Z up, square to it), as "View: new design looks
# along", "Camera direction" and "Sketch left with" log them.
$defaultLook = '-0.568,0.654,-0.5, up -0.328,0.377,0.866; '
$defaultView = '-0.568 0.654 -0.5'
$defaultAlong = '(-0.568, 0.654, -0.500)'
$shownOn = "Design origin shown in the view; the Origin folder's light bulb on"
$hiddenOff = "Design origin hidden in the view; the Origin folder's light bulb off"

# Expect-Origin "state" "description": within six seconds the last "Design
# origin hidden|shown in the view; ..." line is the state (case-sensitive,
# as Expect-OriginOff of the sketch origin test matches it).
function Expect-Origin([string]$State, [string]$Description) {
  $last = $null
  for ($i = 0; $i -lt 30; $i++) {
    Ui-Sync
    $line = Ui-LogLines | Where-Object { $_ -cmatch 'Design origin (hidden|shown) in the view; ' } |
      Select-Object -Last 1
    $last = if ("$line" -cmatch '(Design origin (hidden|shown) in the view; .*)$') { $Matches[1] } else { $null }
    if ($last -ceq $State) { break }
    Start-Sleep -Milliseconds 200
  }
  if ($last -cne $State) { Ui-Fail "${Description}: the last origin line is '$last', expected '$State'" }
  Write-Host "ok   $Description"
}

# Expect-DefaultView "description": the last "View: new design looks along"
# line since the mark looks along the default view with Z up and puts the
# origin within three pixels of the free area's centre, and the camera
# comes to rest along the default view.
function Expect-DefaultView([string]$Description) {
  Ui-ExpectNew 'View: new design looks along ' "${Description}: the new design's view is logged" | Out-Null
  $line = Ui-NewLines | Where-Object { $_.Contains('View: new design looks along ') } | Select-Object -Last 1
  $pattern = 'View: new design looks along ' + [regex]::Escape($defaultLook) +
             "the origin at (-?\d+),(-?\d+), the free area's centre at (-?\d+),(-?\d+)"
  if ("$line" -notmatch $pattern) { Ui-Fail "${Description}: not the default view from the front right and above: '$line'" }
  $dx = [double]$Matches[1] - [double]$Matches[3]
  $dy = [double]$Matches[2] - [double]$Matches[4]
  if ($dx * $dx + $dy * $dy -gt 9) { Ui-Fail "${Description}: the origin is not in the middle of the free area: '$line'" }
  Write-Host "ok   ${Description}: $line"
  Ui-ExpectNew "Camera direction $defaultView up " "${Description}: the camera rests along the default view" | Out-Null
}

# Expect-NoNewDesignFraming "description": an opened design is not framed
# as a new one.
function Expect-NoNewDesignFraming([string]$Description) {
  $framed = Ui-LogLines | Where-Object { $_.Contains('View: new design') } | Select-Object -Last 1
  if ($framed) { Ui-Fail "${Description}: an opened design was framed as a new one: '$framed'" }
  Write-Host "ok   ${Description}: no new design's framing"
}

# Draw-RectangleAndFinish "description": a rectangle from the sketch point
# (-10, -10) to (-40, -30), then Finish Sketch (Ctrl+Enter); the mark is set
# before the finish. Left of and below the origin, away from the orientation
# cube in the view's top right corner: View > Front fits the origin planes
# closer than the default view's fit, and in session 0's 460-pixel view a
# point at (40, 30) lay on the cube (mitcad#1712).
function Draw-RectangleAndFinish([string]$Description) {
  Ui-Mark
  Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
  Ui-ExpectNew 'Sketch tool 2-Point Rectangle' "${Description}: the rectangle tool" | Out-Null
  Ui-Step 'first corner'                   { Ui-SketchClick -10 -10 }
  Ui-Step 'opposite corner'                { Ui-SketchClick -40 -30 }
  Ui-ExpectNew 'Added rectangle' "${Description}: a rectangle" | Out-Null
  Ui-Step 'end the tool (Esc)'             { Ui-Key 'Escape' }
  Ui-Mark
  Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
  Ui-ExpectNew 'Sketch finished' "${Description}: finished" | Out-Null
}

# A box saved with the origin hidden (no display state) and one saved with
# it shown.
function Design([string]$Display) {
  return @"
{
  "format": "mitcad", "version": 2,
  "units": {"length": "mm", "angle": "deg"},
  "parameters": [
    {"name": "d1", "expression": "40 mm", "unit": "mm", "owner": "F1"},
    {"name": "d2", "expression": "25 mm", "unit": "mm", "owner": "F1"},
    {"name": "d3", "expression": "10 mm", "unit": "mm", "owner": "F1"}
  ],
  "features": [
    {"uid": "F1", "name": "Box1", "type": "box", "plane": "xy", "corner": [0, 0],
     "length": "d1", "width": "d2", "height": "d3", "operation": "new_body"}
  ]$Display
}
"@
}
$hiddenFile = Join-Path $script:UiOut 'hidden.mitcad'
$shownFile = Join-Path $script:UiOut 'shown.mitcad'
Set-Content -Encoding ascii $hiddenFile (Design '')
Set-Content -Encoding ascii $shownFile (Design ', "display": {"origin": true}')

Write-Host "--- The untitled design at the start: the default view, the origin shown"
Ui-StartApp 'new-design-start'
Expect-Origin $shownOn 'a new design shows its origin, its light bulb on'
Expect-DefaultView 'the untitled design at the start'
$title = Ui-WindowTitle
if ($title -ne 'Untitled - MitCad') { Ui-Fail "the new design is modified: '$title'" }
Write-Host "ok   showing the origin is no change to the design: '$title'"

Write-Host "--- Create Sketch in the default view: every origin plane picked by a click on it"
foreach ($plane in @('yz', 'xz', 'xy')) {
  Ui-CreateSketch 'F7' $plane
  $pickTurn = Ui-NewLines | Where-Object { $_.Contains('View: Create Sketch turns') } | Select-Object -Last 1
  if ($pickTurn) { Ui-Fail "the view was turned for the plane pick: '$pickTurn'" }
  Draw-RectangleAndFinish "the sketch on $plane"
  Ui-ExpectNew "Sketch left with the view before it: the view looks along $defaultAlong" `
    "the sketch on $plane returns to the default view" | Out-Null
}

Write-Host "--- The view turned to Front before the first sketch keeps Front"
Ui-StartApp 'new-design-start-front'
Expect-DefaultView 'the untitled design at the start, again'
Ui-Mark
Ui-Step 'front view (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'Camera direction 0 1 0' 'looking from the front' | Out-Null
Ui-CreateSketch 'F7' 'xz'
Draw-RectangleAndFinish 'a sketch on XZ from Front'
Ui-ExpectNew 'Sketch left with the view before it: the view looks along (0.000, 1.000, 0.000)' `
  'the sketch returns to Front, the view the user chose' | Out-Null

Write-Host "--- A design saved with its origin hidden"
Ui-StartApp 'new-design-start-hidden' @($hiddenFile)
Expect-Origin $hiddenOff "the opened design's origin stays hidden, its light bulb off"
Ui-ExpectLog "Camera direction $defaultView up " 'the opened design is seen from the default view'
Expect-NoNewDesignFraming 'the design saved with its origin hidden'

Write-Host "--- A design saved with its origin shown, turned to Top, then New Design"
Ui-StartApp 'new-design-start-shown' @($shownFile)
Expect-Origin $shownOn "the opened design's origin stays shown, its light bulb on"
Expect-NoNewDesignFraming 'the design saved with its origin shown'
Ui-Mark
Ui-Step 'top view (F12)'                  { Ui-Key 'F12' }
Ui-ExpectNew 'Camera direction 0 0 -1' 'the opened design seen from the top' | Out-Null
Ui-Mark
Ui-Step 'new design (Ctrl+N)'            { Ui-Key 'ctrl+n' }
Ui-ExpectNew 'New design in ' 'New Design replaced the opened design' | Out-Null
Expect-DefaultView 'New Design after a design turned to Top'
Expect-Origin $shownOn 'the new design shows its origin, its light bulb on'
$title = Ui-WindowTitle
if ($title -ne 'Untitled - MitCad') { Ui-Fail "the new design is modified: '$title'" }
Write-Host "ok   the new design is unmodified: '$title'"

Ui-Finish 'new-design-start'
