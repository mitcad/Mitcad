# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework app/view tools/cli
# Input-driven Windows UI test of the Browser's Rows background in the
# Floating layout (mitcad#419) in the build VM (tools/ui-windows-lib.ps1 says
# how the input is posted); the Linux test is tools/ui-browser-rows-test.sh:
#   1. A 200 x 200 mm plate (mitcad-cli), seen from the top and zoomed in
#      until it fills the view; the Browser starts on its card (set in the
#      settings: Rows is the default on Windows, mitcad#429).
#   2. Preferences > Display > Browser background: Rows (Alt+B, Down, OK):
#      applied at once and saved; a place beside the rows is logged.
#   3. A click there selects the plate's face behind the rows; a click on
#      Body1's row still selects the body; a middle drag beside the rows
#      orbits (the middle-orbit navigation scheme).
#   4. The card follows its rows (mitcad#425; 30 sketches more, Sketches
#      folded at the start, no Projects card): folding Bodies shrinks it,
#      opening it grows it, and the rows do not scroll while it ends above
#      the timeline; opening Sketches grows it down to the top of the
#      message band's row above the timeline (mitcad#1042; the timeline's
#      top without a message) less the gap, the rows scroll, and the
#      scroll bar is at the card's left, left of the rows.
#   5. Back to Card: the place is gone and the plate is not picked there.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-rows-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser-rows'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.preferences.display' = 'F8'; 'view.top' = 'F7' } -StepPause 0
# The middle button orbits (the drag beside the rows). No Projects card:
# the Browser's card starts below the floating rows and has the room down
# to the timeline. The Browser on its card, for the choice of Rows (the
# default on Windows, mitcad#429).
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
Add-Content -Encoding ascii $ini @('[view]', 'navigationScheme=middle-orbit', '[projects]', 'panel=false',
  '[appearance]', 'browserBackground=card')

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# A 200 x 200 x 10 mm plate centred on the origin: Body1 (F2.b0); and 30
# empty sketches more, so that the Browser's rows overflow with Sketches
# open (mitcad#425).
$script = Join-Path $work 'plate.json'
$design = Join-Path $work 'plate.mitcad'
$plateJson = @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
'@
$moreSketches = @(1..30 | ForEach-Object { '  , {"cmd": "sketch.create"}' })
Set-Content -Encoding ascii $script (@($plateJson) + $moreSketches + @(']'))
$output = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# Set-Background card|rows Up|Down: Preferences > Display (F8), the Browser
# background one entry up or down (Alt+B), OK.
function Set-Background([string]$Background, [string]$Key) {
  Ui-Mark
  Ui-Step 'Preferences: Display (F8)'       { Ui-Key 'F8' }
  Ui-ExpectNew 'Preferences opened: Display, ' 'Preferences on its Display page'
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'the Browser background (Alt+B)'  { Ui-Key 'alt+b' }
  Ui-Step "Browser background: $Background" { Ui-Key $Key }
  Ui-Step 'OK (Enter)'                      { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Preferences: browser background $Background" 'the choice is saved'
  Ui-ExpectNew "Browser background: $Background" 'the Browser follows at once'
  if (-not (Select-String -Quiet -Path $ini -Pattern "^browserBackground=$Background$")) {
    Ui-Fail "appearance/browserBackground is not $Background in the settings"
  }
  Write-Host "ok   the setting is $Background"
}

# Since-Mark "text": the lines logged since the mark that contain the text.
function Since-Mark([string]$Text) {
  return @(Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) })
}

# Card-Rect name: @(x, y, width, height) of the card in the last "Floating
# cards:" line (window coordinates).
function Card-Rect([string]$Name) {
  $line = [string](Ui-LogLines | Where-Object { $_.StartsWith('Floating cards: ') } | Select-Object -Last 1)
  $match = [regex]::Match($line, "; $([regex]::Escape($Name)) (-?\d+),(-?\d+) (\d+)x(\d+)")
  if (-not $match.Success) { Ui-Fail "no $Name in '$line'" }
  return @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value, [int]$match.Groups[3].Value,
           [int]$match.Groups[4].Value)
}

# Column-Floor: where the left column ends, plus the gap: the top of the
# message band's row (mitcad#1042), or of the timeline while no message
# was shown.
function Column-Floor {
  $line = [string](Ui-LogLines | Where-Object { $_.StartsWith('Floating cards: ') } | Select-Object -Last 1)
  $match = [regex]::Match($line, '; messageBandRow (-?\d+),(-?\d+) ')
  if ($match.Success) { return [int]$match.Groups[2].Value }
  return (Card-Rect 'timelineDock')[1]
}

# Browser-HeightBecomes lt|gt height description: waits until the Browser
# card's height is less or greater than the height; returns the new one.
function Browser-HeightBecomes([string]$Relation, [int]$Height, [string]$Description) {
  $now = -1
  for ($i = 0; $i -lt 25; $i++) {
    $now = (Card-Rect 'browserDock')[3]
    if (($Relation -eq 'lt' -and $now -lt $Height) -or ($Relation -eq 'gt' -and $now -gt $Height)) {
      Write-Host "ok   $Description ($Height -> $now)"
      return $now
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the Browser card is $now high, not $Relation $Height"
}

# Scroll-Range: the rows' last logged vertical scroll range.
function Scroll-Range { return [int](Ui-LastMatch '^Browser scroll range (\d+)$') }

# Place-Valid "text": whether the last line about it gives its place
# ("<text> at x,y") and not "<text> out of sight".
function Place-Valid([string]$Text) {
  $last = [string](Ui-LogLines | Where-Object { $_.Contains("$Text at ") -or $_.Contains("$Text out of sight") } |
    Select-Object -Last 1)
  return ($last -ne '' -and -not $last.EndsWith(' out of sight'))
}

# The default layout: Floating (mitcad#429).
Ui-StartApp 'browser-rows' @('--open', $design) -DefaultChrome
Ui-ExpectLog "Chrome: floating (the platform's default)" 'Floating by default'
Ui-ExpectLog 'plate.mitcad' 'the plate is open'
Ui-ExpectLog 'Browser background: card' 'the Browser starts on its card (the setting)'
# Its 30 sketches do not fit: the design opens folded below the root
# (mitcad#426); Bodies opened again for Body1's row.
Ui-ExpectLog 'Browser opened folded: ' 'the design opens folded'
Ui-ExpectLog 'Browser Root/Sketches/Sketch1 out of sight' 'the sketches folded away'
Ui-ExpectLog 'Browser arrow Root/Bodies at' "the Bodies folder's arrow"
Ui-Mark
Ui-Step 'open Bodies'                       { Ui-ClickLogged 'Browser arrow Root/Bodies' }
Ui-ExpectNew 'Browser Root/Bodies/Body1 at' "Body1's row"

Write-Host '--- The plate fills the view'
Ui-Step 'top view (F7)'                     { Ui-Key 'F7' }
Ui-Step 'fit (F6)'                          { Ui-Key 'F6' }
# Zoomed in about the plate's middle (the body's logged place), which stays
# put: the plate grows on every side and fills the view, also behind the
# Browser. About the view's middle it grew mostly away from the Browser:
# the plate sits in the free area right of the Browser's card, its left
# edge near the view's middle, and stopped short of the rows.
$plate = Ui-LoggedAt 'Body F2.b0'
Ui-Step 'zoom in at the plate''s middle' {
  [UiInput]::Move($script:UiWindow, $plate[0], $plate[1], 0)
  for ($i = 0; $i -lt 14; $i++) {
    Start-Sleep -Milliseconds 60
    [UiInput]::Wheel($script:UiWindow, $plate[0], $plate[1], 1)
  }
}
Ui-WaitIdle

Write-Host '--- Rows'
Set-Background 'rows' 'Down'
Ui-ExpectLog 'Browser view beside the rows at' 'a place beside the rows is logged'
$beside = Ui-LoggedAt 'Browser view beside the rows'
Ui-Mark
Ui-Step 'click beside the rows'             { Ui-ClickLogged 'Browser view beside the rows' }
Ui-ExpectNew 'Selected: 1 face [face F2:' "the click reached the view: the plate's face behind the rows"
Ui-Mark
Ui-Step "click Body1's row"                 { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Selected: 1 body [body F2.b0]' "a row's box still takes the click"

$beside = Ui-LoggedAt 'Browser view beside the rows'
Ui-Mark
Ui-Step 'middle drag beside the rows' { Ui-Drag $beside[0] $beside[1] ($beside[0] + 120) ($beside[1] + 64) 2 }
Ui-ExpectNew 'Camera direction' 'a middle drag beside the rows orbits' 8
$last = "$(Ui-LogLines | Where-Object { $_.Contains('Camera direction') } | Select-Object -Last 1)"
if ($last.Contains('direction 0 0 -1 ')) { Ui-Fail "the drag did not turn the view: $last" }
Write-Host "ok   orbited to: $last"
if ((Since-Mark 'Selected: ').Count -ne 0) { Ui-Fail 'the drag selected something' }
Write-Host 'ok   the drag selected nothing'

Write-Host '--- The card grows with its rows, then scrolls; the scroll bar at the left (mitcad#425)'
$h0 = (Card-Rect 'browserDock')[3]
Ui-Mark
Ui-Step 'fold Bodies'                       { Ui-ClickLogged 'Browser arrow Root/Bodies' }
Ui-ExpectNew 'Browser Root/Bodies/Body1 out of sight' 'Body1 folded away'
$h1 = Browser-HeightBecomes 'lt' $h0 'the card shrinks as a row folds'
Ui-Mark
Ui-Step 'open Bodies'                       { Ui-ClickLogged 'Browser arrow Root/Bodies' }
Ui-ExpectNew 'Browser Root/Bodies/Body1 at' 'Body1 in sight again'
Browser-HeightBecomes 'gt' $h1 'the card grows as a row opens' | Out-Null
$browser = Card-Rect 'browserDock'
$timelineTop = Column-Floor
if ($browser[1] + $browser[3] -ge $timelineTop - 8) {
  Ui-Fail "the card reaches the timeline ($($browser[1]) + $($browser[3]), timeline at $timelineTop)"
}
if ((Scroll-Range) -ne 0) { Ui-Fail "the rows scroll ($(Scroll-Range)) before the card reaches the timeline" }
if (Place-Valid 'Browser scroll bar') { Ui-Fail 'a scroll bar before the card reaches the timeline' }
Write-Host 'ok   no scroll bar while the rows fit'
Ui-Mark
Ui-Step 'open Sketches'                     { Ui-ClickLogged 'Browser arrow Root/Sketches' }
Ui-ExpectNew 'Browser Root/Sketches/Sketch1 at' 'the sketches in sight'
for ($i = 0; $i -lt 25; $i++) {
  $browser = Card-Rect 'browserDock'
  $timelineTop = Column-Floor
  if ($browser[1] + $browser[3] -eq $timelineTop - 8) { break }
  Start-Sleep -Milliseconds 200
}
$bottom = $browser[1] + $browser[3]
if ($bottom -ne $timelineTop - 8) {
  Ui-Fail "the card ends at $bottom, not at the timeline's top less the gap $($timelineTop - 8)"
}
Write-Host "ok   the card grew down to the timeline ($bottom, the timeline at $timelineTop)"
for ($i = 0; $i -lt 25; $i++) {
  if ((Scroll-Range) -gt 0 -and (Place-Valid 'Browser scroll bar')) { break }
  Start-Sleep -Milliseconds 200
}
if ((Scroll-Range) -le 0) { Ui-Fail 'the rows do not scroll' }
if (-not (Place-Valid 'Browser scroll bar')) { Ui-Fail 'no scroll bar once the rows overflow' }
$bar = Ui-LoggedAt 'Browser scroll bar'
$root = Ui-LoggedAt 'Browser Root'
if ($bar[0] -ge $root[0]) { Ui-Fail "the scroll bar at x $($bar[0]) is not left of the rows (Root at $($root[0]))" }
Write-Host "ok   the rows scroll ($(Scroll-Range)); the scroll bar at the card's left ($($bar[0]), Root's name at $($root[0]))"

Write-Host '--- Back to Card'
Set-Background 'card' 'Up'
Ui-ExpectNew 'Browser view beside the rows out of sight' 'the place beside the rows is gone'
Ui-Mark
Ui-Step 'click where the rows left room'    { Ui-Click $beside[0] $beside[1] }
# A moment for a pick that would come, as the Linux test waits.
Start-Sleep -Milliseconds 500
Ui-Sync
# The card's tree may select the row there; the plate behind is not picked.
if ((Since-Mark 'Selected: 1 face').Count -ne 0) { Ui-Fail 'the click went past the card to the view' }
Write-Host 'ok   the card takes the click again'

Ui-Finish 'Windows UI browser rows test'
