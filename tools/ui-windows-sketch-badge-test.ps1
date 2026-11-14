# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchOverlay.cpp app/framework/FloatingLayout.cpp app/framework/Language.cpp
# check-all sources: app/browser/BrowserPanel.cpp app/view/AnnotationsWindow.cpp tools/cli
# Input-driven Windows UI test of sketch mode's badge, its English and the
# annotations while a sketch is edited (mitcad#1756) in the build VM, in
# the Floating layout (tools/ui-windows-lib.ps1 says how the input is
# posted); the Linux test is tools/ui-sketch-badge-test.sh. On a design
# mitcad-cli makes: 30 empty sketches, then Sketch31 (a free rectangle and
# a free line), Extrude1 of its rectangle (Body1, F32.b0) and two material
# labels on Body1's faces (Material1, Material2):
#   1. The Browser card open down to the timeline (its Sketches folder
#      open): editing Sketch31 (its row, double-click) puts its badge and
#      the pointer's place above it clear of every floating card, right of
#      the Browser card, which reaches down past the badge's row; with the
#      pointer in the view too.
#   2. English plurals: the badge says "Sketch31: <n> degrees of freedom",
#      never "degree(s)".
#   3. The edit rolls the timeline back to Sketch31: the labels are rolled
#      back with Extrude1, the Browser's Annotations row never turns a
#      warning, the Annotations window's rows say "Rolled back"; Finish
#      Sketch brings them back as before.
#   4. A Browser row clicked by its logged place after the rows scrolled
#      it away (mitcad#1804): Ui-ClickLogged scrolls it back into sight
#      first and clicks the row, never its old place.
# The rows clicked (Sketch31's, the Annotations folder's, Sketch1's) are
# scrolled into sight by the shared helper (Ui-LoggedAt), as on Linux.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-sketch-badge-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-sketch-badge'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# Any error of the script fails the test and stops the app: an app left
# running keeps ctest's output pipe open, and ctest waited for its time-out
# (mitcad#1787).
trap { Ui-Fail "the test script stopped: $_ $($_.InvocationInfo.PositionMessage)" }

Ui-Init $App $Out $QtBin @{} -StepPause 0
# No Projects card: the Browser's card has the column from the rows down to
# the timeline. The Browser on its card.
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
Add-Content -Encoding ascii $ini @('[projects]', 'panel=false', '[appearance]', 'browserBackground=card')

if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp); pass -Cli" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# 30 empty sketches (F1-F30), so that the Browser's rows overflow with
# Sketches open; Sketch31 (F31) with a rectangle and a line, neither
# constrained; Extrude1 (F32) of the rectangle, Body1 (F32.b0); the labels
# on its top and front faces.
$script = Join-Path $work 'badge.json'
$design = Join-Path $work 'badge.mitcad'
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$emptySketches = @(1..30 | ForEach-Object { '  {"cmd": "sketch.create"},' })
$badgeJson = @'
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F31", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "sketch.add_line", "sketch": "F31", "start": [40, 0], "end": [50, 10]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F31", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_annotation", "kind": "material", "a": {"body": "F32.b0", "face": "F32:end(RECT)"}, "at": [5, 5, 20]},
  {"cmd": "add_annotation", "kind": "material", "a": {"body": "F32.b0", "face": "F32:side(c1[c4,c2])"},
    "at": [10, 0, 10]}
]
'@
Set-Content -Encoding ascii $script (@('[') + $emptySketches + @($badgeJson.Replace('RECT', $rect)))
$output = & $Cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# Last-Line "start": the last log line that starts with the text ('' if none).
# "$(...)", not [string](...): Windows PowerShell casts an empty pipeline to
# $null, not '' (mitcad#1787).
function Last-Line([string]$Start) {
  return "$(Ui-LogLines | Where-Object { $_.StartsWith($Start) } | Select-Object -Last 1)"
}

# Card-Rect name: @(x, y, width, height) of the card in the last "Floating
# cards:" line (window coordinates).
function Card-Rect([string]$Name) {
  $line = Last-Line 'Floating cards: '
  $match = [regex]::Match($line, "; $([regex]::Escape($Name)) (-?\d+),(-?\d+) (\d+)x(\d+)")
  if (-not $match.Success) { Ui-Fail "no $Name in '$line'" }
  return @([int]$match.Groups[1].Value, [int]$match.Groups[2].Value, [int]$match.Groups[3].Value,
           [int]$match.Groups[4].Value)
}

# Badge: the last "Sketch badge" line as @(bx, by, bw, bh, px, py, pw, ph):
# the badge's box and the pointer's place's (window coordinates); $null
# when none is logged.
function Badge {
  $line = Last-Line "Sketch badge '"
  $match = [regex]::Match($line,
    "' at (-?\d+),(-?\d+) (\d+)x(\d+), pointer place (-?\d+),(-?\d+) (\d+)x(\d+), view ")
  if (-not $match.Success) { return $null }
  return @(1..8 | ForEach-Object { [int]$match.Groups[$_].Value })
}

# Covering x y w h: the floating cards of the last "Floating cards:" line
# that the rectangle overlaps ("name x,y wxh; ..."); '' when none.
function Covering([int]$X, [int]$Y, [int]$W, [int]$H) {
  $line = Last-Line 'Floating cards: '
  $hits = ''
  foreach ($entry in @($line -split '; ' | Select-Object -Skip 1)) {
    $match = [regex]::Match($entry, '(-?\d+),(-?\d+) (\d+)x(\d+)$')
    if (-not $match.Success) { continue }
    $cx = [int]$match.Groups[1].Value; $cy = [int]$match.Groups[2].Value
    $cw = [int]$match.Groups[3].Value; $ch = [int]$match.Groups[4].Value
    if ($X -lt $cx + $cw -and $cx -lt $X + $W -and $Y -lt $cy + $ch -and $cy -lt $Y + $H) { $hits += "$entry; " }
  }
  return $hits
}

# Expect-Clear description: within five seconds the badge and the
# pointer's place overlap no floating card (the cards and the badge follow
# each other); returns the badge's place (Badge).
function Expect-Clear([string]$Description) {
  $hit = 'no badge logged'
  for ($i = 0; $i -lt 25; $i++) {
    $b = Badge
    if ($null -ne $b) {
      $hit = (Covering $b[0] $b[1] $b[2] $b[3]) + (Covering $b[4] $b[5] $b[6] $b[7])
      if ($hit -eq '') {
        Write-Host "ok   ${Description}: the badge at $($b[0]),$($b[1]) $($b[2])x$($b[3]), the pointer's place at $($b[4]),$($b[5]) $($b[6])x$($b[7]), under no card"
        return $b
      }
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: the badge or the pointer's place lies under a card: $hit ($(Last-Line "Sketch badge '"))"
}

Ui-StartApp 'sketch-badge' @('--open', $design, '--chrome=floating')
Ui-ExpectLog 'badge.mitcad' 'the design is open'
Ui-ExpectLog 'Annotations: Material1 material | Material2 material' 'both labels listed, none broken'

Write-Host '--- The Browser card down to the timeline'
# Its 31 sketches do not fit: the design opens folded below the root
# (mitcad#426); Sketches opened again, the card grows down to the timeline.
Ui-ExpectLog 'Browser opened folded: ' 'the design opens folded'
Ui-ExpectLog 'Browser arrow Root/Sketches at' "the Sketches folder's arrow"
Ui-Sync
Ui-Mark
Ui-Step 'open Sketches'                     { Ui-ClickLogged 'Browser arrow Root/Sketches' }
Ui-ExpectNew 'Browser Root/Sketches/Sketch1 at' 'the sketches in sight'
Ui-Sync

Write-Host '--- Edit Sketch31: the badge clear of the cards (mitcad#1756)'
Ui-Mark
Ui-Step 'edit Sketch31 (its Browser row, double-click)' { Ui-DoubleClickLogged 'Browser Root/Sketches/Sketch31' }
Ui-ExpectNew 'Editing sketch F31 (Sketch31)' 'Sketch31 edited' 20
Ui-ExpectNew "Sketch badge 'Sketch31: " 'the badge placed'
Ui-Sync
$badge = Expect-Clear 'the pointer outside the view'
$browser = Card-Rect 'browserDock'
# The case of the report: the Browser card reaches down past the badge's
# row, and the badge is right of it.
if ($browser[1] + $browser[3] -le $badge[1]) {
  Ui-Fail "the Browser card ($($browser -join ' ')) ends above the badge's row (y $($badge[1])): open its rows down to the timeline"
}
if ($badge[0] -lt $browser[0] + $browser[2]) {
  Ui-Fail "the badge (x $($badge[0])) is not right of the Browser card ($($browser -join ' '))"
}
Write-Host "ok   right of the Browser card ($($browser -join ' ')), which reaches past the badge's row"

Write-Host '--- English plurals (mitcad#1756)'
$text = [regex]::Match((Last-Line "Sketch badge '"), "^Sketch badge '([^']*)'").Groups[1].Value
if ($text -notmatch '^Sketch31: \d+ degrees of freedom$') {
  Ui-Fail "the badge says '$text', not 'Sketch31: <n> degrees of freedom'"
}
Write-Host "ok   the badge says '$text'"
$bracketed = @(Ui-LogLines | Where-Object { $_.Contains('degree(s)') })
if ($bracketed.Count -ne 0) { Ui-Fail "a text says 'degree(s)': $($bracketed[0])" }
Write-Host "ok   no 'degree(s)' anywhere in the log"

Write-Host "--- The pointer in the view: its place above the badge, clear of the cards too"
Ui-Mark
$inView = Ui-ViewAt 60 50
# MK_SHIFT in wParam keeps the posted move in session 0: Qt drops a move
# with nothing in it as the "enter" of a window shown under the cursor
# while no window is under the pointer (there is no cursor there), and
# reads the modifiers from the key state, so it is a plain move
# (tools/ui-windows-build-test.ps1, mitcad#1787).
Ui-Step 'pointer into the view'             { [UiInput]::Move($script:UiWindow, $inView[0], $inView[1], 0x04) }
Ui-ExpectNew 'Sketch pointer x ' "the pointer's place logged"
Ui-Sync
$null = Expect-Clear 'the pointer in the view'

Write-Host '--- The labels rolled back with Extrude1, no warning'
Ui-ExpectLog 'Annotations: Material1 rolled back: ' 'Material1 rolled back with the timeline'
if (-not (Last-Line 'Annotations: ').Contains('| Material2 rolled back: ')) {
  Ui-Fail "Material2 is not rolled back: $(Last-Line 'Annotations: ')"
}
Write-Host 'ok   Material2 rolled back too'
Ui-Mark
Ui-Step "Annotations window (the Browser's folder, double-click)" { Ui-DoubleClickLogged 'Browser Root/Annotations' }
Ui-FocusDialog '^Annotations$'
Ui-ExpectNew 'Annotations row Material1: material, Rolled back, ' "Material1's row: rolled back"
Ui-ExpectNew 'Annotations row Material2: material, Rolled back, ' "Material2's row: rolled back"
Ui-Step 'close the window (Escape)'         { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- Finish Sketch: as before'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'        { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'Sketch31 finished'
Ui-ExpectNew 'Annotations: Material1 material | Material2 material' 'both labels back, none broken'
Ui-Sync
$warned = @(Ui-LogLines | Where-Object { $_ -match '^Browser level .*/Annotations: warning' -or
  $_ -match '^Annotations level Material[12]: warning' })
if ($warned.Count -ne 0) { Ui-Fail "a warning on the Annotations row or a label: $($warned[0])" }
Write-Host 'ok   no warning on the Annotations row or a label, while editing or after'

Write-Host "--- A row clicked by its logged place after the rows scrolled it away (mitcad#1804)"
# Sketch1's row in sight first (the shared helper scrolls it there), then
# the rows wheeled down until the app logs it out of sight: its last "at"
# place is where another row is now. The click by its logged place
# scrolls it back and selects Sketch1, not the row at its old place.
Ui-BrowserReveal 'Browser Root/Sketches/Sketch1'
if (-not (Ui-PlaceValid 'Browser Root/Sketches/Sketch1')) { Ui-Fail "Sketch1's row is not in sight" }
$old = Ui-LoggedAt 'Browser Root/Sketches/Sketch1'
for ($i = 0; $i -lt 8 -and (Ui-PlaceValid 'Browser Root/Sketches/Sketch1'); $i++) {
  if (-not (Ui-BrowserWheel -4)) { Ui-Fail 'no Browser row in sight to wheel over' }
}
$gone = "$(Ui-PlaceLine 'Browser Root/Sketches/Sketch1')"
if (-not $gone.EndsWith(' out of sight')) { Ui-Fail "Sketch1's row did not scroll out of sight: '$gone'" }
Write-Host "ok   Sketch1's row scrolled away from $($old[0]),$($old[1]) ('$gone')"
Ui-Mark
Ui-Step "click Sketch1's row by its logged place" { Ui-ClickLogged 'Browser Root/Sketches/Sketch1' }
Ui-ExpectNew 'Browser selected: Root/Sketches/Sketch1' 'Sketch1 selected: its row scrolled back and clicked'
$selected = Ui-LastMatch '^Browser selected: (.*)$'
if ($selected -ne 'Root/Sketches/Sketch1') { Ui-Fail "the click selected '$selected', not Sketch1 alone" }
if (-not (Ui-PlaceValid 'Browser Root/Sketches/Sketch1')) { Ui-Fail "Sketch1's row is not in sight after the click" }
Write-Host "ok   Sketch1 alone selected, its row in sight at $((Ui-LoggedAt 'Browser Root/Sketches/Sketch1') -join ',')"

Ui-Finish 'Windows UI sketch badge test'
