# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp tools/cli core/model/src/build core/model-build core/model-api/src/build.rs
# check-all sources: tools/ui-windows-timber-lib.ps1
# Input-driven Windows UI test of frames of timber members and slot
# profiles with the BUILD tab's Timber and Profile (mitcad#485, mitcad#492),
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted),
# with the synthetic library of tools/ui-windows-timber-lib.ps1 (mitcad#834:
# once part of tools/ui-windows-timber-test.ps1):
#   1. A frame of four members with shared mitres (mitcad#485): each on the
#      end of the one before, turned at a corner, lying flat as it does
#      (ends of the same width); the last one's auto length bound to the
#      first closes the corner with a shared mitre, as long as the first
#      one's outer side.
#   2. The same frame of slot profiles with Profile, the last one's length
#      through the first: a shared mitre, not into it.
#   3. The timber frame again, the last one's length typed to the first's
#      outer side: it closes the corner with a shared mitre too (mitcad#492).
#   4. A board along X on the ground rests on it (its axis 24 mm up); a
#      member standing on its top face from end A at offset 0 lies as it
#      does: its width along the board's, its sides flush (mitcad#492).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-timber-frames-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-timber-frames'),
  [string]$QtBin = '')

$TimberTest = 'UI Windows timber frames test'
. (Join-Path $PSScriptRoot 'ui-windows-timber-lib.ps1')
Timber-Init

# Frame-Of-Four CMD KEY PART L1 L2 [-Through]: a frame on the XY plane, L1
# along X and L2 across, of four members (PART) each on the end of the
# one before with a shared mitre (mitcad#485): the first along X at the
# origin, the second turned to -Y, the third to -X; the view then from the
# front left (the third's end faces -X), the fourth turned to +Y, its auto
# length bound to the first (Tab-stepped to "through" with THROUGH), or
# with TYPED its length typed as L2 (mitcad#492): it closes the corner with
# a shared mitre, L2 long.
function Frame-Of-Four([string]$Cmd, [string]$Key, [string]$Part, [string]$L1, [string]$L2, [switch]$Through,
                       [switch]$Typed) {
  Ui-Mark
  Ui-Step "$Cmd ($Key)"                  { Ui-Key $Key }
  Ui-ExpectNew "Command $Cmd started" "$Cmd started"
  Ui-Step "$L1 mm long"                  { Ui-TypeIn "Panel $Cmd input length" $L1 }
  Ui-Mark
  Ui-Step 'along X'                      { Ui-ClickLogged "Panel $Cmd input axis=x" }
  Expect-Inspect $Cmd @("place grounded, length $L1.00 mm (typed)", '; along x') "the first along X"
  Place $Part
  $m1 = Placed-Body
  $m1Name = Ui-LastMatch '^Placed .* as (.*) \(O[0-9/]*\), length'
  if (-not $m1) { Ui-Fail 'no body of the first member' }
  Corner $Cmd $m1 '-Y' $L2
  if ($Cmd -eq 'Timber') {
    Expect-Inspect $Cmd @('place end', '; thickness along Z') 'on the end lying flat as the first: ends of the same width'
  }
  Place $Part
  $m2 = Placed-Body
  Corner $Cmd $m2 '-X' $L1
  Place $Part
  $m3 = Placed-Body
  # The third one's end faces -X: the view from the front left (the cube's
  # places logged, MITCAD_LOG_ORIENTATION_CUBE).
  Ui-Step 'the view from the front left' { Ui-ClickLogged 'Orientation cube front-left-top' }
  # The camera at rest once the step's sync answered (its rest timer is
  # watched; mitcad#870: a fixed 800 ms more before).
  Ui-Sync
  Corner $Cmd $m3 '+Y' $(if ($Typed) { $L2 } else { '' })
  if ($Through) {
    Ui-Step 'to the centre (Tab)'        { Ui-Key 'Tab' }
    Ui-Mark
    Ui-Step 'through (Tab)'              { Ui-Key 'Tab' }
    Ui-ExpectNew '(bound, auto: through to ' 'bound through the first'
  }
  Expect-Inspect $Cmd @("length $L2.00 mm", "; closes the corner at end A of $m1Name, shared mitre") 'the last one closes the corner with a shared mitre, as long as the side'
  Place $Part
  if (-not $Typed) {
    Ui-ExpectNew 'length bound, auto: ' 'placed with its length bound to the first'
  }
  Write-Host "ok   a frame of $Cmd closed: $m1, $m2, $m3 and the last one"
  Ui-Step 'Close (Escape)'               { Ui-Key 'Escape' }
}

Write-Host '--- 1. A frame of four timber members, mitred'
Ui-StartApp 'timber-frame'
Ui-Step 'the BUILD tab'                  { Ui-ClickTab 'BUILD' }
Frame-Of-Four 'Timber' 'F4' 'Timber 48x98 test' '400' '300'

Write-Host '--- 2. A frame of four slot profiles, mitred, the last one through the first'
Ui-StartApp 'timber-profile-frame'
Ui-Step 'the BUILD tab'                  { Ui-ClickTab 'BUILD' }
Frame-Of-Four 'Profile' 'F3' 'Profile 20 test' '200' '150' -Through

Write-Host '--- 3. A frame of four timber members, the last one typed to the first one''s outer side'
Ui-StartApp 'timber-typed-frame'
Ui-Step 'the BUILD tab'                  { Ui-ClickTab 'BUILD' }
Frame-Of-Four 'Timber' 'F4' 'Timber 48x98 test' '400' '300' -Typed

Write-Host '--- 4. A board on the ground rests on it; one standing on its top face lies as it does'
Ui-StartApp 'timber-lie'
Ui-Step 'the BUILD tab'                  { Ui-ClickTab 'BUILD' }
Ui-Mark
Ui-Step 'Timber (F4)'                    { Ui-Key 'F4' }
Ui-ExpectNew 'Command Timber started' 'Timber started'
Ui-Step '600 mm long'                    { Ui-TypeIn 'Panel Timber input length' '600' }
Ui-Mark
Ui-Step 'along X'                        { Ui-ClickLogged 'Panel Timber input axis=x' }
Expect-Inspect 'Timber' @('place grounded, length 600.00 mm (typed)', '; along x, flat', '; thickness along Z',
                          '; end A at 0.0, 0.0, 24.0') 'flat along X, resting on the XY plane: its axis 24 mm up'
Place 'Timber 48x98 test'
$board = Placed-Body
if (-not $board) { Ui-Fail 'no body of the board' }
Refresh-Picks 'at'
# Its section's side at +Y (the sketch's line c8) faces up when it lies flat.
$tops = @(Faces-Of $board ':part(F2:side(c8[')
if ($tops.Count -eq 0) { Ui-Fail "no top face of the board ($board) to pick" }
Ui-Mark
Ui-Step "the board's top face"           { Pick-Face $board $tops[0] }
Ui-ExpectNew 'Inspect Timber: place side' 'on its top face'
Ui-Mark
Ui-Step 'from end A'                     { Wheel-Choice 'Timber' 'from' -2 }
Ui-Step 'offset 0'                       { Ui-TypeIn 'Panel Timber input offset' '0' }
# Standing up from end A at offset 0, not rotated: its 48 along the board,
# its 98 across the board's 98, its sides flush (it came turned a quarter).
Expect-Inspect 'Timber' @('place side', ' at 0.0 mm from a', '; thickness along X', '; end A at 24.0, 0.0, 48.0') `
  'standing on the top face from end A at 0: its width along the board''s width, flush'
Place 'Timber 48x98 test'
Ui-Step 'Close (Escape)'                 { Ui-Key 'Escape' }

Ui-Finish $TimberTest
