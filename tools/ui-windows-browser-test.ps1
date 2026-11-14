# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/print app/MainWindow.cpp tools/cli app/MainWindowBrowserHover.cpp
# Input-driven Windows UI test of the browser's scrolling with a selection
# (mitcad#318) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted); the Linux test is the last part of tools/ui-browser-test.sh:
#   1. A design with 60 bodies and Plane1 above them (mitcad-cli): more
#      rows than the browser shows at once, Plane1's at the bottom.
#   2. Plane1 picked in the view: the tree scrolls to its row.
#   3. Body1 clicked at the top, the rows scrolled down with the wheel until
#      Body1 is out of sight, Body60 Ctrl+clicked: both bodies selected and
#      the scroll position stays where it was.
#   4. Five light bulbs clicked quickly with a small mouse move during each
#      press (mitcad#344): each toggles once and the selection is unchanged
#      (the Linux test is a part of tools/ui-browser-test.sh).
#   5. Material > More... and Appearance... of Body60's row, Body1 and
#      Body60 selected, start the Material and the Appearance tool for both
#      bodies (mitcad#703, mitcad#1059; the Linux test, which assigns, is
#      tools/ui-material-test.sh). Material > Acme PETG, a
#      material of the design, assigns it to both without a window and
#      Ctrl+Z takes it back in one step (mitcad#1046; the Linux test is
#      tools/ui-material-menu-test.sh).
#   6. In a small assembly, the pointer over a body's and a component's row
#      highlights their bodies in the view and the highlight goes with the
#      pointer; nothing is meshed, the selection stays (mitcad#1022; a step
#      of tools/ui-browser-test.sh).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-browser-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-browser'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin -StepPause 0

# Browser-Scroll: where the rows are scrolled to now.
function Browser-Scroll { return Ui-LastMatch '^Browser scroll (\d+)$' }

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# Body1..Body60 (F2..F61, all from Sketch1's rectangle) and Plane1 (F62)
# 60 mm above them.
$rect = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$lines = @('[', '  {"cmd": "sketch.create"},',
  '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},')
for ($i = 1; $i -le 60; $i++) {
  $lines += '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
    '"}], "extent": {"type": "distance", "distance": ' + ($i % 10 + 1) + '}, "operation": "new_body"}},'
}
$lines += '  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 60}}},'
# A material of the design's own for Material > (mitcad#1046).
$lines += '  {"cmd": "create_material", "id": "acme_petg", "name": "Acme PETG", "density": 1.27, "appearance": "plastic_orange"}'
$lines += ']'
$script = Join-Path $work 'many.json'
$design = Join-Path $work 'many.mitcad'
Set-Content -Encoding ascii $script $lines
$output = & $cli run $script --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

Ui-StartApp 'browser' @('--open', $design)
Ui-ExpectLog 'many.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
# A tree longer than the browser opens folded below the root (mitcad#426):
# Construction, then Bodies are opened (Bodies' place stays where it is).
Ui-ExpectLog 'Browser opened folded: ' 'the long tree opened folded'
Ui-Sync
Ui-Step 'open Construction'              { Ui-ClickLogged 'Browser arrow Root/Construction' }
Ui-Step 'open Bodies'                    { Ui-ClickLogged 'Browser arrow Root/Bodies' }
Ui-ExpectLog 'Browser Root/Bodies/Body1 at' 'the rows from the top'
# Plane1's pick place is logged once the camera rests in the laid-out view
# (the one logged when the design opened, before the view had its size, is
# 0,0): the last one is a place in the view.
$plane = $null
for ($i = 0; $i -lt 50; $i++) {
  $plane = Ui-LastMatch '^Datum F62 at (\d+,\d+)$'
  if ($plane -and $plane -ne '0,0') { break }
  Start-Sleep -Milliseconds 200
}
if (-not $plane -or $plane -eq '0,0') { Ui-Fail "Plane1's place in the view is not logged ('$plane')" }
Write-Host "ok   Plane1 in the view at $plane"
if (Ui-PlaceValid 'Browser Root/Construction/Plane1') { Ui-Fail 'every row in sight: nothing to scroll' }
Write-Host 'ok   Plane1''s row is below the rows in sight'

Write-Host '--- A pick in the view scrolls the tree to its row'
Ui-Mark
Ui-Step 'pick Plane1 in the view'        { Ui-ClickLogged 'Datum F62' }
Ui-ExpectNew 'Selected: 1 plane [plane F62]' 'Plane1 picked in the view'
Ui-ExpectNew 'Browser scrolled to Root/Construction/Plane1' 'the tree scrolled to its row'
Ui-ExpectNew 'Browser Root/Construction/Plane1 at' 'its row in sight'
Ui-ExpectNew 'Browser selected: Root/Construction/Plane1' 'and selected'

Write-Host '--- Ctrl+click below, the first selected row scrolled away (mitcad#318)'
Ui-Mark
Ui-Step 'click Body1'                    { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Selected: 1 body [body F2.b0]' 'Body1 selected'
# The row's selection logged before it is read below, as the Linux test waits.
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1' 'its row selected'
Write-Host '--- Quick clicks on light bulbs (mitcad#344)'
# Five light bulbs clicked one after the other with no pause, the mouse
# moving a little while each button is down: each toggles its row once and
# the selection stays Body1 (the view's pressed row must not start a range).
function Quick-Eyes([string[]]$Paths) {
  $places = @()
  foreach ($path in $Paths) { $places += , (Ui-LoggedAt "Browser eye $path") }
  foreach ($p in $places) {
    # Apart from a press there a moment ago (the rows are further apart,
    # so the round itself does not wait).
    Ui-Apart $script:UiWindow $p[0] $p[1]
    [UiInput]::Move($script:UiWindow, $p[0], $p[1], 0)
    [UiInput]::Button($script:UiWindow, 0x201, $p[0], $p[1], 0x01)
    [UiInput]::Move($script:UiWindow, $p[0] + 2, $p[1] + 1, 0x01)
    [UiInput]::Button($script:UiWindow, 0x202, $p[0] + 2, $p[1] + 1, 0)
  }
  Ui-Sync
}
$quick = @('Root/Bodies/Body2', 'Root/Bodies/Body3', 'Root/Bodies/Body4', 'Root/Bodies/Body5', 'Root/Bodies/Body6')
$selected = Ui-LastMatch '^Browser selected: (.*)$'
Ui-Mark
Ui-Step 'click five light bulbs quickly' { Quick-Eyes $quick }
foreach ($path in $quick) { Ui-ExpectNew "Visibility ${path}: hidden" "$path hidden" }
$new = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
$toggles = @($new | Where-Object { $_.StartsWith('Visibility ') }).Count
if ($toggles -ne 5) { Ui-Fail "five quick clicks gave $toggles visibility changes" }
if ((Ui-LastMatch '^Browser selected: (.*)$') -ne $selected) { Ui-Fail 'the quick clicks changed the selection' }
Write-Host "ok   each light bulb toggled once, the selection is unchanged ($selected)"
Ui-Mark
Ui-Step 'click them again' { Quick-Eyes $quick }
foreach ($path in $quick) { Ui-ExpectNew "Visibility ${path}: shown" "$path shown again" }
if ((Ui-LastMatch '^Browser selected: (.*)$') -ne $selected) { Ui-Fail 'the second round changed the selection' }

# Body60's row wheeled into sight (Ui-BrowserReveal, which Ui-LoggedAt runs too).
Ui-BrowserReveal 'Browser Root/Bodies/Body60'
if (-not (Ui-PlaceValid 'Browser Root/Bodies/Body60')) { Ui-Fail "Body60's row cannot be scrolled into sight" }
if (Ui-PlaceValid 'Browser Root/Bodies/Body1') { Ui-Fail 'Body1''s row is still in sight' }
$before = Browser-Scroll
if (-not $before -or [int]$before -le 0) { Ui-Fail "the rows are not scrolled down ('$before')" }
Write-Host "ok   scrolled down to $before, Body1 out of sight"
$at = Ui-LoggedAt 'Browser Root/Bodies/Body60'
Ui-Mark
Ui-Step 'Ctrl+click Body60'              { Ui-ClickWith 'ctrl' $at[0] $at[1] }
Ui-ExpectNew 'Selected: 2 bodies' 'two bodies selected'
Ui-ExpectNew 'body F2.b0' 'Body1 still selected'
Ui-ExpectNew 'body F61.b0' 'and Body60'
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1, Root/Bodies/Body60' 'both rows selected'
Ui-Sync
$after = Browser-Scroll
if ($after -ne $before) { Ui-Fail "Ctrl+click scrolled the tree from $before to $after" }
if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Browser scrolled to') }) {
  Ui-Fail 'the tree scrolled to a row'
}
if (-not (Ui-PlaceValid 'Browser Root/Bodies/Body60')) { Ui-Fail 'Body60''s row left the sight' }
Write-Host "ok   the scroll position stays at $after"

Write-Host '--- Material... and Appearance... of a selected row are for every selected body (mitcad#703)'
# Menu-Choose "entry" ["entry of its submenu"]: the entry of the context
# menu logged last ("Context menu: A | B"), chosen with the keys as the
# menu opens with nothing active (the labels' ellipsis matches three dots);
# with a second entry the first opens a submenu (Return, its first entry
# active), whose entries are logged as "Context menu <entry>: C | D" when
# it opens (Material >, mitcad#1046).
$ellipsis = [string][char]0x2026
function Menu-Choose([string]$Entry, [string]$Sub = '') {
  $entries = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ' | ForEach-Object { $_.Replace($ellipsis, '...') })
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  $from = @(Ui-LogLines).Count
  for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
  Ui-Key 'Return'
  if (-not $Sub) { return }
  $line = $null
  for ($i = 0; $i -lt 30 -and -not $line; $i++) {
    $line = Ui-LogLines | Select-Object -Skip $from | Where-Object { $_.StartsWith("Context menu ${Entry}: ") } |
      Select-Object -Last 1
    if (-not $line) { Start-Sleep -Milliseconds 200 }
  }
  if (-not $line) { Ui-Fail "the submenu $Entry did not open" }
  $subEntries = @("$line".Substring("Context menu ${Entry}: ".Length) -split ' \| ' |
    ForEach-Object { $_.Replace($ellipsis, '...') })
  $subIndex = [array]::IndexOf($subEntries, $Sub)
  if ($subIndex -lt 0) { Ui-Fail "'$Sub' is not in the submenu ${Entry}: $($subEntries -join ' | ')" }
  for ($i = 0; $i -lt $subIndex; $i++) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
# Expect-NewMatch "regex" "description": Ui-ExpectNew with a regular
# expression (the two bodies in either order).
function Expect-NewMatch([string]$Pattern, [string]$Description) {
  for ($i = 0; $i -lt 30; $i++) {
    if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_ -match $Pattern }) {
      Write-Host "ok   $Description"
      return
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: /$Pattern/ not in the log since the mark"
}
# Changed-Selection: a "Browser selected" line since the mark.
function Changed-Selection {
  return [bool](Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.StartsWith('Browser selected: ') })
}
$both = '(F2\.b0, F61\.b0|F61\.b0, F2\.b0)'
# Select-Both: Body1 and Body60 selected again (a tool takes the selection
# it starts with): Body1's row scrolled into sight and clicked, then
# Body60's Ctrl+clicked.
function Select-Both {
  Ui-Mark
  Ui-Step 'click Body1'                  { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
  Ui-ExpectNew 'Browser selected: Root/Bodies/Body1' 'Body1 selected'
  $place = Ui-LoggedAt 'Browser Root/Bodies/Body60'
  Ui-Step 'Ctrl+click Body60'            { Ui-ClickWith 'ctrl' $place[0] $place[1] }
  Ui-ExpectNew 'Browser selected: Root/Bodies/Body1, Root/Bodies/Body60' 'both rows selected again'
}
Ui-Mark
Ui-Step 'right-click Body60'             { Ui-ClickLogged 'Browser Root/Bodies/Body60' 3 }
Ui-ExpectNew 'Context menu: ' 'the body''s context menu'
Ui-Step 'choose Material > More...'      { Menu-Choose 'Material' 'More...' }
Expect-NewMatch "^Material tool for bodies $both$" 'the Material tool for both selected bodies'
Ui-ExpectNew 'Panel Material bodies note: 2 bodies: Steel' 'the panel notes both bodies'
Ui-Step 'cancel the Material tool (Escape)' { Ui-Key 'Escape' }
Select-Both
Ui-Mark
Ui-Step 'right-click Body60 again'       { Ui-ClickLogged 'Browser Root/Bodies/Body60' 3 }
Ui-ExpectNew 'Context menu: ' 'the body''s context menu'
Ui-Step 'choose Appearance...'           { Menu-Choose 'Appearance...' }
Expect-NewMatch "^Appearance tool for $both$" 'the Appearance tool for both selected bodies'
Ui-Step 'cancel the Appearance tool (Escape)' { Ui-Key 'Escape' }
Select-Both

Write-Host '--- Material > a design material: both selected bodies, one undo step (mitcad#1046)'
Ui-Mark
Ui-Step 'right-click Body60 once more'   { Ui-ClickLogged 'Browser Root/Bodies/Body60' 3 }
Ui-ExpectNew 'Context menu: ' 'the body''s context menu'
Ui-Step 'choose Material > Acme PETG'    { Menu-Choose 'Material' 'Acme PETG' }
Ui-ExpectNew 'Context menu Material checked: Steel' 'Steel, what both bodies are made of, is checked'
Ui-ExpectNew 'Material assigned from menu: Acme PETG to 2 bodies' 'one pick assigns Acme PETG to both bodies'
if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.StartsWith('Materials opened') }) {
  Ui-Fail 'the pick opened the Materials window'
}
if (Changed-Selection) { Ui-Fail 'the pick changed the selection' }
Write-Host 'ok   no window opened, the selection kept'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Material' 'one undo step gives both bodies their material back'
Ui-Mark
Ui-Step 'right-click Body60 after undo'  { Ui-ClickLogged 'Browser Root/Bodies/Body60' 3 }
Ui-ExpectNew 'Context menu: ' 'the body''s context menu'
Ui-Step 'open Material'                  { Menu-Choose 'Material' }
Ui-ExpectNew 'Context menu Material checked: Steel' 'both are Steel again'
Ui-Step 'close the submenu and the menu' { Ui-Key 'Escape'; Ui-Key 'Escape' }

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- The pointer over a row highlights its bodies in the view (mitcad#1022)'
# Body1 (F2.b0) in the root and Component1:1 (O1) with Body2, 40 mm along
# X, as in tools/ui-browser-test.sh, whose step this is.
# The two extrudes as whole lines first: inside @(...) Windows PowerShell 5.1
# ended an element at a line that closes with "+", so the region was lost.
$bodyLine = '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
  '"}], "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},'
$componentLine = '  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "' + $rect +
  '"}], "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},'
$assembly = @('[', '  {"cmd": "sketch.create"},',
  '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},',
  $bodyLine, $componentLine,
  '  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}}',
  ']')
$assemblyScript = Join-Path $work 'assembly.json'
$assemblyDesign = Join-Path $work 'assembly.mitcad'
Set-Content -Encoding ascii $assemblyScript $assembly
$output = & $cli run $assemblyScript --save $assemblyDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $assemblyDesign)) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
Ui-StopApp
Ui-StartApp 'browser-hover' @('--open', $assemblyDesign)
Ui-ExpectLog 'assembly.mitcad' 'the assembly opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Bodies/Body1 at' 'the browser lists Body1'
Ui-ExpectLog 'Browser Root/Component1:1 at' 'and Component1:1'
Ui-Sync
# Hover-Row "Browser <path>" "<path>" "description": the pointer over the
# row, then the row's highlight and, in session 0, its end: a posted move
# leaves no cursor over the browser, which gets its Leave right after (as
# the joint preview, tools/ui-windows-joint-highlight-test.ps1). MK_SHIFT
# in wParam keeps the move (Qt drops a plain move while no window is under
# the pointer; it reads the modifiers from the key state).
function Hover-Row([string]$Text, [string]$Row, [string]$Description) {
  Ui-Mark
  Ui-Step "point at $Row" {
    $at = Ui-LoggedAt $Text
    [UiInput]::Move($script:UiWindow, $at[0], $at[1], 0x04)
  }
  Ui-ExpectNew "Browser hover highlight: $Row (1 bodies), " $Description
  for ($i = 0; $i -lt 30; $i++) {
    $lines = @(Ui-LogLines | Select-Object -Skip $script:UiMark)
    $shown = @(0..($lines.Count - 1) | Where-Object { $lines[$_].StartsWith("Browser hover highlight: $Row ") })
    if ($shown.Count -gt 0 -and @($lines | Select-Object -Skip ($shown[0] + 1) |
        Where-Object { $_.StartsWith('Browser hover highlight: none (0 bodies), ') }).Count -gt 0) {
      Write-Host 'ok   the highlight goes with the pointer'
      return
    }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "the highlight of $Row does not go with the pointer: no 'Browser hover highlight: none' after it"
}
$selected = Ui-LastMatch '^Browser selected: (.*)$'
$from = (Ui-LogLines).Count
Hover-Row 'Browser Root/Bodies/Body1' 'Root/Bodies/Body1' 'the body''s row highlights it'
Hover-Row 'Browser Root/Component1:1' 'Root/Component1:1' 'the component''s row highlights its body'
Ui-Sync
$new = @(Ui-LogLines | Select-Object -Skip $from)
if ($new | Where-Object { $_ -match '^Browser hover highlight: .*, [1-9][0-9]* faces meshed$' }) { Ui-Fail 'a hover meshed faces' }
if ($new | Where-Object { $_ -match '^(Selected: |Browser selected: |View body highlights: |Visibility )' }) {
  Ui-Fail 'the hover changed the selection or the visibility'
}
if ((Ui-LastMatch '^Browser selected: (.*)$') -ne $selected) { Ui-Fail 'the selection changed with the hover' }
Write-Host 'ok   nothing meshed; the selection and the visibility unchanged'

Ui-Finish 'UI Windows browser test'
