# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework app/browser app/view core/model-api/src tools/cli
# Input-driven Windows UI test of measurement objects (mitcad#649), now
# annotations (mitcad#702), in the build VM (tools/ui-windows-lib.ps1 says
# how the input is posted), as tools/ui-measurement-objects-test.sh on
# Linux, on the design of tools/ui-windows-measure-test.ps1: two cylinders
# of radius 5, 20 high (Body1 about Z through the origin, Body2 through
# (30, 40)) and a block (Body3).
#   1. Keep as Dimension in the Measure panel: the round faces kept as
#      AxisDistance1; the Annotate menu's entries, its Annotations...
#      opening the window; Close (mitcad#1010).
#   2. Keep as Dimension in a history row's menu: Body1's top rim shown
#      again from its row's menu (Show) and kept as Diameter1.
#   3. Export Snapshot (F9 here) at 4 x the view, transparent: a PNG of
#      four times the view's size with an alpha channel.
#   4. Saved, opened again: both measured and drawn.
# tools/ui-windows-annotations-test.ps1 edits them.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-measurement-objects-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-measurement-objects'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'view.export_snapshot' = 'F9' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelJson = Join-Path $work 'model.json'
$design = Join-Path $work 'axes.mitcad'
Set-Content -Encoding ascii $modelJson @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 40], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [20, -30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F5", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
'@
$output = & $cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

# Wait-Enabled "place": until the last "<place> at x,y" line is not
# " (disabled)": the button's place logged once it is enabled.
function Wait-Enabled([string]$Place) {
  $last = $null
  for ($i = 0; $i -lt 30; $i++) {
    $last = Ui-LogLines | Where-Object { $_.Contains("$Place at ") } | Select-Object -Last 1
    if ($last -and -not "$last".EndsWith('(disabled)')) { return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "'$Place' is not enabled: $last"
}

# The labels' ellipsis as three dots, for the log's lines.
$ellipsis = [string][char]0x2026
# Menu-Entries: the entries of the context menu logged last.
function Menu-Entries {
  return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ' | ForEach-Object { $_.Replace($ellipsis, '...') })
}
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step "choose $Entry" {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}
# Annotate-Entries: the Annotate menu's entries that can be chosen, as
# logged last.
function Annotate-Entries { return "$(Ui-LastMatch '^Measure Annotate entries: (.*)$')".Replace($ellipsis, '...') }

Ui-StartApp 'measurement-objects' @('--open', $design)
Ui-ExpectLog 'Annotations: none' 'no annotations yet'
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F2.b0' 'the cylinders shown'

Write-Host '--- 1. Keep as Dimension in the Measure panel'
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
Ui-ExpectNew 'Command Measure started' 'measure started'
Ui-ExpectNew 'Measure Keep as Dimension at' 'the panel has Keep as Dimension'
Ui-ExpectNew 'Measure Annotate at' 'and Annotate beside it'
Ui-Step 'pick Body1''s round face'         { Ui-ClickPick 'face F2.b0/F2:side(c1)' }
Ui-ExpectNew 'Measure Annotate entries: Note' 'one face picked: the leaders can be chosen'
if ((Annotate-Entries) -ne 'Note..., Datum Label..., Frame..., Hole Callout, Annotations...') {
  Ui-Fail "not every Annotate entry for a face: $(Annotate-Entries)"
}
Write-Host 'ok   every Annotate entry for a face'
Ui-Step 'pick Body2''s round face'         { Ui-ClickPick 'face F4.b0/F4:side(c1)' }
Ui-ExpectNew 'distance 40, angle 0 deg, axis distance 50, axis lines 2' 'the axes 50 apart'
Ui-ExpectNew 'Measure Keep as Dimension enabled' 'Keep as Dimension enabled for the model''s picks'
Wait-Enabled 'Measure Keep as Dimension'
if ((Annotate-Entries) -ne 'Annotations...') { Ui-Fail "two picks, yet a leader can be chosen: $(Annotate-Entries)" }
Write-Host 'ok   two picks: no leader, only Annotations...'
# At the panel's default width (mitcad#1010): no control's text clipped,
# every result row on one line.
Ui-Sync
$width = [int](Ui-LastMatch '^Panel Measure width (\d+) of')
if ($width -le 0 -or $width -gt 300) { Ui-Fail "the panel is $width wide, not within the default 300" }
if ("$(Ui-LastLineWith 'Measure controls')" -ne 'Measure controls: none clipped') {
  Ui-Fail "a control's text is clipped at ${width}: $(Ui-LastLineWith 'Measure controls')"
}
if ("$(Ui-LastLineWith 'Panel Measure inputs')" -ne 'Panel Measure inputs: none clipped') {
  Ui-Fail "a pick's label or the filter's text is clipped at ${width}: $(Ui-LastLineWith 'Panel Measure inputs')"
}
if ("$(Ui-LastLineWith 'Panel Measure result: ')" -notmatch 'rows, one line each$') {
  Ui-Fail "a result row does not fit its line: $(Ui-LastLineWith 'Panel Measure result: ')"
}
Write-Host "ok   no control clipped, every result row on one line at $width"
Ui-Mark
Ui-Step 'Keep as Dimension'                { Ui-ClickLogged 'Measure Keep as Dimension' }
Ui-ExpectNew 'Measure Keep as Dimension clicked' 'Keep as Dimension clicked'
Ui-ExpectNew 'Annotation kept: AxisDistance1' 'kept as the axes'' distance'
Ui-ExpectNew 'Annotations drawn: AxisDistance1 = 50 (auto)' 'drawn in Auto'
Ui-Mark
Ui-Step 'Annotate'                         { Ui-ClickLogged 'Measure Annotate' }
Ui-ExpectNew 'Measure Annotate clicked' 'Annotate clicked'
Ui-ExpectNew 'Context menu: ' 'its menu'
if (((Menu-Entries) -join ' | ') -ne 'Note... | Datum Label... | Frame... | Hole Callout | Annotations...') {
  Ui-Fail "not the Annotate menu's entries: $((Menu-Entries) -join ' | ')"
}
Write-Host 'ok   the leaders, then Annotations...'
Ui-Step 'Annotations... (A)'               { Ui-Key 'a' }
Ui-ExpectNew 'Measure Annotations clicked' 'Annotations... chosen'
Ui-ExpectNew 'Annotations window opened: 1 annotation(s)' 'the window opened from the panel'
Ui-Step 'close the window (Escape)'        { Ui-FocusDialog '^Annotations$'; Ui-Key 'Escape'; Ui-FocusMain }
Ui-Mark
Ui-Step 'Close'                            { Ui-ClickLogged 'Panel Measure Close' }
Ui-ExpectNew 'Closed Measure' 'measure closed'
Ui-ExpectLog 'Browser Root/Annotations at' 'the browser has the Annotations folder'

Write-Host '--- 2. Keep as Dimension in a history row''s menu'
Ui-Mark
Ui-Step 'measure (I)'                      { Ui-Key 'i' }
Ui-Step 'pick Body1''s top rim'            { Ui-ClickPick 'edge F2.b0/E{F2:end(r{c1})|' }
Ui-ExpectNew 'Inspect Measure: length 31.416, radius 5, diameter 10' 'the rim measured'
Ui-ExpectNew 'Measure history row 2: Edge of Body1' 'its row in the history'
Ui-ExpectNew 'Measure history at' 'the history''s header laid out'
Ui-Step 'unfold the history'               { Ui-ClickLogged 'Measure history' }
Ui-ExpectNew 'Measure history opened' 'unfolded'
Ui-ExpectNew 'Measure history row 2 at' 'its row laid out'
Ui-Mark
Ui-Step 'right-click the rim''s row'       { Ui-ClickLogged 'Measure history row 2' 3 }
Ui-ExpectNew 'Context menu: Show | Keep as Dimension | Copy' 'its menu'
Menu-Choose 'Show'
Ui-ExpectNew 'Measure history showed row 2' 'shown again'
Ui-Sync
Ui-Mark
Ui-Step 'right-click the rim''s row'       { Ui-ClickLogged 'Measure history row 2' 3 }
Ui-ExpectNew 'Context menu: Show | Keep as Dimension | Copy' 'its menu again'
Menu-Choose 'Keep as Dimension'
Ui-ExpectNew 'Measure history Keep as Dimension clicked: row 2' 'Keep as Dimension chosen for the row'
Ui-ExpectNew 'Annotation kept: Diameter1' 'kept as the rim''s diameter'
Ui-ExpectNew "Annotations drawn: AxisDistance1 = 50 (auto) | Diameter1 = $([char]0x00D8)10 (auto)" 'both drawn'
Ui-Step 'close (Enter)'                    { Ui-Key 'Return' }
Ui-ExpectNew 'Closed Measure' 'measure closed'

Write-Host '--- 3. Export Snapshot: 4 x the view, transparent'
$area = Ui-LastMatch 'View area (\d+ \d+ \d+ \d+)'
if (-not $area) { Ui-Fail 'the app did not log its view area' }
$view = $area -split ' ' | ForEach-Object { [int]$_ }
$width = 4 * $view[2]
$height = 4 * $view[3]
$png = Join-Path $work 'snapshot.png'
Ui-Mark
Ui-Step 'export snapshot (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Snapshot dialog opened: size 2x, transparent off, annotations all (2 shown)' 'the dialog with its defaults'
Ui-Step 'type the file' {
  Ui-FocusDialog '^Export Snapshot$'
  Ui-Key 'ctrl+a'
  Ui-Type $png
}
Ui-Step 'Size: 4 x (Alt+Z, Down)'         { Ui-Key 'alt+z'; Ui-Key 'Down' }
Ui-ExpectNew 'Snapshot dialog: size = 4x' 'four times the view'
Ui-Step 'Transparent background (Alt+T)'  { Ui-Key 'alt+t' }
Ui-ExpectNew 'Snapshot dialog: transparent = on' 'a transparent background'
Ui-Step 'OK (Enter)'                      { Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew "Snapshot saved ${png}: $width x $height, transparent, 2 annotation(s)" 'the snapshot written' 30
if (-not (Test-Path $png)) { Ui-Fail 'no snapshot file' }
$bytes = [System.IO.File]::ReadAllBytes($png)
$pw = ([int]$bytes[16] -shl 24) -bor ([int]$bytes[17] -shl 16) -bor ([int]$bytes[18] -shl 8) -bor [int]$bytes[19]
$ph = ([int]$bytes[20] -shl 24) -bor ([int]$bytes[21] -shl 16) -bor ([int]$bytes[22] -shl 8) -bor [int]$bytes[23]
if ($pw -ne $width -or $ph -ne $height) { Ui-Fail "the snapshot is $pw x $ph, not $width x $height" }
if ($bytes[25] -ne 6) { Ui-Fail "the snapshot has no alpha channel (PNG colour type $($bytes[25]))" }
Write-Host "ok   snapshot.png is $pw x $ph with an alpha channel, $($bytes.Length) bytes"

Write-Host '--- 4. Saved and opened again'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                   { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
if (-not (Select-String -Path $design -Pattern '"annotations"' -Quiet)) { Ui-Fail 'the design file has no annotations' }
Ui-StartApp 'measurement-objects-again' @('--open', $design)
Ui-ExpectLog 'Annotations: AxisDistance1 axis_distance | Diameter1 diameter' 'both read back'
Ui-ExpectLog "Annotations drawn: AxisDistance1 = 50 (auto) | Diameter1 = $([char]0x00D8)10 (auto)" 'and drawn'

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'UI Windows measurement objects test'
