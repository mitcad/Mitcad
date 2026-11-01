# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files app/browser/BrowserController.cpp app/framework/NoteRow.cpp
# check-all sources: app/framework/CommandPanel.cpp core/model core/3mf core/zip tools/cli
# Input-driven Windows UI test of purge volumes on filament changes
# (mitcad#705) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), as tools/ui-purge-volumes-test.sh does on Linux, on a design
# of two boxes, one of a white filament and one of a black one:
#   1. 3D Print (a shortcut of the test's) with 3MF: the Purge volumes row
#      says Computed; its pencil opens the table of the two filaments with
#      the computed volumes (white to black 140 mm3, black to white 420 mm3:
#      dark to light takes more).
#   2. A cell typed: the pair's own, marked as set by hand; the row says
#      1 set by hand, the other pair stays computed. Closed, the panel
#      cancelled (the edit stays), saved: the design file keeps the pair.
#   3. 3D Print again: the row and the table show the pair set by hand;
#      Reset of the selected cell gives the computed volume again, the row
#      says Computed; closed, cancelled, saved: the file has no pair left.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-purge-volumes-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-purge-volumes'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.print3d' = 'F8' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The user's library of materials and profiles in the test's own folder.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $work 'print-library'

# The log's arrow and cube sign, as characters: the script stays ASCII
# (Windows PowerShell reads a script without a byte order mark in the
# system's code page).
$to = ' ' + [string][char]0x2192 + ' '
$mm3 = ' mm' + [string][char]0x00B3
$whiteToBlack = "Purge volume White PLA${to}Black PETG = "
$blackToWhite = "Purge volume Black PETG${to}White PLA = "

# Two 20 mm boxes: the first of a white PLA (the root's material), the
# second of a black PETG. Their colours give the purge values: white 210
# into and 70 out of it, black 70 into and 210 out of it.
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'boxes.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "white-pla", "name": "White PLA", "based_on": "pla",
   "filament": {"filament_colour": "#FFFFFF"}},
  {"cmd": "create_material", "id": "black-petg", "name": "Black PETG", "based_on": "pla",
   "filament": {"filament_type": "PETG", "filament_colour": "#000000"}},
  {"cmd": "set_component_material", "component": "C0", "material": "white-pla"},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "black-petg"}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli (the design): $($made -join ' / ')" }

# 3MF, set so that the panel starts with it whatever the default.
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[print]', 'format=3mf')

# Click-InPurge "text": a click on a logged place of the Purge Volumes
# window. The places are logged in the main window's coordinates, as the
# others are, but a click posted to the main window stays in the main
# window's widgets: this one goes to the window Ui-FocusDialog chose, at
# the place moved by the two windows' client origins.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PurgeClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InPurge([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object PurgeClient+Point
  $dialog = New-Object PurgeClient+Point
  [PurgeClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [PurgeClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

# The design's purge pairs as saved: "from>to=volume" each, the volume as
# a number (the file may write 300 or 300.0).
function Saved-Pairs {
  $saved = Get-Content -Raw -Encoding UTF8 $design | ConvertFrom-Json
  $inv = [Globalization.CultureInfo]::InvariantCulture
  return @($saved.purge_volumes | Where-Object { $_ } |
    ForEach-Object { "$($_.from)>$($_.to)=$(([double]$_.volume).ToString($inv))" }) -join ', '
}

# Close-AndSave: the table closed, the panel cancelled (its edits stay in
# the design), the design saved.
function Close-AndSave {
  Ui-Step 'close'                        { Click-InPurge 'Purge volumes close' }
  Ui-FocusMain
  Ui-Mark
  Ui-Step 'cancel the panel (Esc)'       { Ui-Key 'Escape' }
  Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed'
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Saved ' 'saved'
}

Ui-StartApp 'purge-volumes' @('--open', $design)

Write-Host '--- 1. 3D Print: the Purge volumes row and its table, computed'
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
Ui-ExpectNew '; format 3mf, ' 'the panel starts with 3MF'
Ui-ExpectNew 'Panel Send to Slicer note purge: Computed [Edit]' 'the row says the volumes are computed'
Ui-ExpectNew 'Panel Send to Slicer action purge.0 at ' 'its pencil has its place'
Ui-PanelReveal 'Panel Send to Slicer input purge'
Ui-Mark
Ui-Step 'the pencil'                     { Ui-ClickLogged 'Panel Send to Slicer action purge.0' }
Ui-ExpectNew 'Purge volumes opened: White PLA, Black PETG' 'the table of the two filaments, in the order of use'
Ui-ExpectNew "${whiteToBlack}140${mm3}" 'white to black: 70 out of white, 70 into black'
Ui-ExpectNew "${blackToWhite}420${mm3}" 'black to white: 210 and 210, dark to light takes more'
Ui-ExpectNew 'Purge volumes cell 0.1 at ' 'the cells have their places'
Ui-ExpectNew 'Purge volumes close at ' 'Close has its place'
$early = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('(set by hand)') }
if ($early) { Ui-Fail "a pair is set by hand already: $($early -join ' / ')" }
Write-Host 'ok   no pair set by hand yet'
Ui-FocusDialog '^Purge Volumes$'

Write-Host '--- 2. A cell typed: set by hand, marked, saved'
Ui-Mark
Ui-Step 'White PLA to Black PETG'        { Click-InPurge 'Purge volumes cell 0.1' }
Ui-Step 'type 300, Enter'                { Ui-Type '300'; Ui-Key 'Return' }
Ui-ExpectNew "Purge volume set: White PLA${to}Black PETG = 300${mm3}" 'Enter set it'
Ui-ExpectNew "${whiteToBlack}300${mm3} (set by hand)" 'the cell is marked as set by hand'
Ui-ExpectNew 'Panel Send to Slicer note purge: 1 set by hand [Edit]' 'the row counts it'
$other = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($blackToWhite) } |
  Select-Object -Last 1
if ("$other".Contains('set by hand')) { Ui-Fail "the other pair changed: $other" }
Write-Host 'ok   the other pair stays computed'
Close-AndSave
$pairs = Saved-Pairs
if ($pairs -ne 'white-pla>black-petg=300') { Ui-Fail "the design's purge pairs: '$pairs', expected white-pla>black-petg=300" }
Write-Host 'ok   the design keeps the pair'

Write-Host '--- 3. Reset: computed again'
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer note purge: 1 set by hand [Edit]' 'the row shows the pair set by hand'
Ui-ExpectNew 'Panel Send to Slicer action purge.0 at ' 'its pencil has its place'
Ui-PanelReveal 'Panel Send to Slicer input purge'
Ui-Mark
Ui-Step 'the pencil'                     { Ui-ClickLogged 'Panel Send to Slicer action purge.0' }
Ui-ExpectNew 'Purge volumes opened: White PLA, Black PETG' 'the table opened again'
Ui-ExpectNew "${whiteToBlack}300${mm3} (set by hand)" 'the pair is still set by hand'
# A window shown anew logs its places again.
Ui-ExpectNew 'Purge volumes cell 0.1 at ' 'the cells have their places again'
Ui-ExpectNew 'Purge volumes reset at ' 'Reset has its place'
Ui-FocusDialog '^Purge Volumes$'
Ui-Mark
Ui-Step 'White PLA to Black PETG'        { Click-InPurge 'Purge volumes cell 0.1' }
Ui-Step 'Reset'                          { Click-InPurge 'Purge volumes reset' }
Ui-ExpectNew 'Purge volumes reset: 1' "Reset took the pair's own volume away"
Ui-ExpectNew "${whiteToBlack}140${mm3}" 'the computed volume again'
Ui-ExpectNew 'Panel Send to Slicer note purge: Computed [Edit]' 'the row says computed again'
foreach ($pair in @($whiteToBlack, $blackToWhite)) {
  $last = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($pair) } | Select-Object -Last 1
  if ("$last".Contains('set by hand')) { Ui-Fail "a pair is still set by hand: $last" }
}
Write-Host 'ok   no pair set by hand'
Close-AndSave
$pairs = Saved-Pairs
if ($pairs) { Ui-Fail "the design still has purge pairs: '$pairs'" }
Write-Host 'ok   the design has no pair left'

Ui-Finish 'UI Windows purge volumes test'
