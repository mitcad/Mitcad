# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs tools/cli
# Input-driven Windows UI test of My Library (mitcad#704) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-my-library-test.sh does on Linux: the user's materials and
# printers managed without a design, on a design of one box that must not
# change. My library is the test's own folder (MITCAD_PRINT_LIBRARY_DIR),
# which mitcad-cli gives a printer, Bench X.
#   0. Edit Materials (a shortcut of the test's), Edit Library of the
#      Materials window (after Delete, left of Close, mitcad#1019): My
#      Library's first opening logs its parts' times ("My
#      Library timing: ..."), the panels fill when the items were read off
#      the UI thread ("My Library loaded: ..."; mitcad#1018). The times are
#      printed, none asserted.
#   1. My Library (a shortcut of the test's): Duplicate of the built-in
#      PLA Plastic makes PLA Plastic (copy) in My library (its file), renamed
#      Bench PLA, its density 1.3, each written to the file; Delete asks
#      and removes the file.
#   2. The Printers tab: Duplicate of Bench X makes Bench X (copy) in My
#      library, renamed Bench Y; Delete removes it, Bench X stays.
#   3. The design did not change: no modified mark in the title, its file
#      as it was.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-my-library-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-my-library'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.my_library' = 'F8'; 'solid.materials' = 'F9' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# The user's library of materials and profiles in the test's own folder.
$library = Join-Path $work 'print-library'
$env:MITCAD_PRINT_LIBRARY_DIR = $library

# Ui-ExpectItem path field expected description: a field of a library
# item's file (`material.name`, `profile.values.max_print_height`).
function Ui-ExpectItem([string]$Path, [string]$Field, [string]$Expected, [string]$Description) {
  if (-not (Test-Path $Path)) { Ui-Fail "${Description}: no $Path" }
  $value = Get-Content -Raw $Path | ConvertFrom-Json
  foreach ($part in $Field.Split('.')) { $value = $value.$part }
  # Numbers as numbers: the file may write 250 or 250.0.
  $inv = [Globalization.CultureInfo]::InvariantCulture
  $a = 0.0; $b = 0.0
  $numbers = [double]::TryParse("$value", [Globalization.NumberStyles]::Float, $inv, [ref]$a) -and
             [double]::TryParse($Expected, [Globalization.NumberStyles]::Float, $inv, [ref]$b)
  if (($numbers -and $a -ne $b) -or (-not $numbers -and "$value" -ne $Expected)) {
    Ui-Fail "${Description}: $Field is '$value', expected '$Expected'"
  }
  Write-Host "ok   $Description"
}

# Ui-LoggedAfter "text": the last line since the mark with the text.
function Ui-LoggedAfter([string]$Text) {
  return Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) } | Select-Object -Last 1
}

# The design: one box. My library: a printer.
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'box.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli (the design): $($made -join ' / ')" }
$printerScript = Join-Path $work 'printer.json'
Set-Content -Encoding ascii $printerScript @'
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench X",
   "values": {"bed_shape": [[0, 0], [220, 0], [220, 220], [0, 220]], "max_print_height": 250,
              "nozzle_diameter": 0.4}}
]
'@
$made = & $Cli run $printerScript --save (Join-Path $work 'other.mitcad')
$benchX = Join-Path $library 'print\printers\bench-x.json'
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $benchX)) { Ui-Fail "mitcad-cli (Bench X into My library): $($made -join ' / ')" }
$designSum = (Get-FileHash $design).Hash

# Click-InLibrary "text": a click on a logged place of the My Library
# window. The places are logged in the main window's coordinates, as the
# others are, but a click posted to the main window stays in the main
# window's widgets: this one goes to the window Ui-FocusDialog chose, at
# the place moved by the two windows' client origins.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class LibraryClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InLibrary([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object LibraryClient+Point
  $dialog = New-Object LibraryClient+Point
  [LibraryClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [LibraryClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

Ui-StartApp 'my-library' @('--open', $design)

Write-Host '--- 0. My Library from the Materials window: its first opening, timed (mitcad#1018)'
Ui-Mark
Ui-Step 'Edit Materials (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Materials opened' 'the Materials window opened'
Ui-ExpectNew 'Materials timing: ' "the Materials window logs its opening's time"
Ui-ExpectNew 'Materials edit_library at ' 'Edit Library has its place'
Ui-ExpectNew 'Materials close at ' 'Close has its place'
# Edit Library (mitcad#1019): after Delete in the row under the list, left
# of Close; no result button.
$editAt = Ui-LoggedAt 'Materials edit_library'
$deleteAt = Ui-LoggedAt 'Materials delete'
$closeAt = Ui-LoggedAt 'Materials close'
if ($editAt[1] -ne $deleteAt[1] -or $editAt[1] -ne $closeAt[1] -or $editAt[0] -le $deleteAt[0] -or $editAt[0] -ge $closeAt[0]) {
  Ui-Fail "Edit Library at $($editAt -join ',') is not after Delete ($($deleteAt -join ',')) and before Close ($($closeAt -join ','))"
}
Write-Host 'ok   Edit Library after Delete, left of Close'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'Edit Library...'                { Click-InLibrary 'Materials edit_library' }
Ui-ExpectNew 'Materials: Edit Library' 'the button'
Ui-ExpectNew 'My Library opened' 'My Library opened'
Ui-ExpectNew 'My Library timing: ' "its first opening logs each part's time"
Ui-ExpectNew 'My Library loaded: ' 'the panels filled when the items were read' 60
# The times as logged, for the record; none is asserted.
foreach ($text in @('Materials timing: ', 'My Library timing: ', 'My Library loaded: ')) {
  $line = Ui-LogLines | Where-Object { $_.Contains($text) } | Select-Object -Last 1
  Write-Host "     $($line.Substring($line.IndexOf($text)))"
}
Ui-FocusDialog '^My Library$'
Ui-Mark
Ui-Step 'close My Library (Esc)'         { Ui-Key 'Escape' }
Ui-ExpectNew 'My Library closed' 'closed'
Ui-FocusDialog '^Materials$'
Ui-Step 'close Materials (Esc)'          { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 1. My Library: a built-in material duplicated, renamed, its density changed, deleted'
Ui-Mark
Ui-Step 'My Library (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'My Library opened' 'the window opened'
Ui-ExpectNew 'My Library materials row: fdm_plastics/pla PLA Plastic (mitcad)' 'PLA is listed, built in'
Ui-ExpectNew 'My Library tab printers at ' 'the tabs have their places'
Ui-ExpectNew 'My Library materials item PLA Plastic at ' 'its row has its place'
Ui-FocusDialog '^My Library$'
Ui-Mark
Ui-Step 'PLA Plastic'                    { Click-InLibrary 'My Library materials item PLA Plastic' }
Ui-ExpectNew 'Material selected: PLA Plastic (mitcad)' 'PLA is selected'
Ui-ExpectNew 'My Library materials duplicate at ' 'Duplicate under the list'
Ui-Step 'Duplicate'                      { Click-InLibrary 'My Library materials duplicate' }
Ui-ExpectNew 'Material duplicated: PLA Plastic as ' 'a copy made'
$line = Ui-LoggedAfter 'Material duplicated: PLA Plastic as '
if ($line -notmatch ' as (\S+) in (\S+)$') { Ui-Fail "no id in: $line" }
$id = $Matches[1]
if ($Matches[2] -ne 'user') { Ui-Fail "the copy went to '$($Matches[2])', not My library" }
Write-Host 'ok   in My library, not in the design'
$material = Join-Path $library "materials\$id.json"
Ui-ExpectItem $material 'material.name' 'PLA Plastic (copy)' 'its file, named after PLA'
Ui-Sync
Ui-Mark
Ui-Step 'type the name'                  { Ui-Type 'Bench PLA'; Ui-Key 'Return' }
Ui-ExpectNew 'Material PLA Plastic (copy) (user) name = Bench PLA' 'Enter renamed it'
Ui-ExpectItem $material 'material.name' 'Bench PLA' 'the file has the new name'
Ui-ExpectNew 'My Library materials density at ' 'Density has its place'
Ui-Step 'Density'                        { Click-InLibrary 'My Library materials density'; Start-Sleep -Milliseconds 200; Ui-Key 'ctrl+a'; Ui-Type '1.3'; Ui-Key 'Return' }
Ui-ExpectNew 'Material Bench PLA (user) density = 1.3' 'Enter saved the density'
Ui-ExpectItem $material 'material.density' '1.3' 'the file has the new density'
Ui-Mark
Ui-Step 'Delete'                         { Click-InLibrary 'My Library materials delete' }
Ui-ExpectNew 'My Library materials delete: Delete Bench PLA from My library?' 'Delete asks first, naming the library'
Ui-FocusDialog '^Delete$'
Ui-Step 'Delete (Alt+D)'                 { Ui-Key 'alt+d' }
Ui-ExpectNew 'Material deleted: Bench PLA (user)' 'deleted'
if (Test-Path $material) { Ui-Fail 'the file is still there' }
Write-Host 'ok   its file is gone'

Write-Host '--- 2. Printers: Bench X duplicated, renamed, deleted'
Ui-FocusDialog '^My Library$'
Ui-Mark
Ui-Step 'the Printers tab'               { Click-InLibrary 'My Library tab printers' }
Ui-ExpectNew 'My Library tab: printers' 'the Printers tab'
Ui-ExpectNew 'My Library printers row: printer/bench-x Bench X (user)' 'Bench X is listed'
Ui-ExpectNew 'My Library printers item Bench X at ' 'its row has its place'
Ui-Step 'Bench X'                        { Click-InLibrary 'My Library printers item Bench X' }
Ui-ExpectNew 'Profile selected: printer Bench X (user)' 'Bench X is selected'
Ui-ExpectNew 'My Library printers duplicate at ' 'Duplicate under the list'
Ui-Mark
Ui-Step 'Duplicate'                      { Click-InLibrary 'My Library printers duplicate' }
Ui-ExpectNew 'Profile duplicated: Bench X as ' 'a copy made'
$line = Ui-LoggedAfter 'Profile duplicated: Bench X as '
if ($line -notmatch ' as (\S+) in (\S+)$') { Ui-Fail "no id in: $line" }
$printerId = $Matches[1]
if ($Matches[2] -ne 'user') { Ui-Fail "the copy went to '$($Matches[2])', not My library" }
$printer = Join-Path $library "print\printers\$printerId.json"
Ui-ExpectItem $printer 'profile.name' 'Bench X (copy)' 'its file, named after Bench X'
Ui-ExpectItem $printer 'profile.values.max_print_height' '250' "with Bench X's values"
Ui-Sync
Ui-Mark
Ui-Step 'type the name'                  { Ui-Type 'Bench Y'; Ui-Key 'Return' }
Ui-ExpectNew 'Profile Bench X (copy) (user) name = Bench Y' 'Enter renamed it'
Ui-ExpectItem $printer 'profile.name' 'Bench Y' 'the file has the new name'
Ui-Mark
Ui-Step 'Delete'                         { Click-InLibrary 'My Library printers delete' }
Ui-ExpectNew 'My Library printers delete: Delete Bench Y from My library?' 'Delete asks first'
Ui-FocusDialog '^Delete$'
Ui-Step 'Delete (Alt+D)'                 { Ui-Key 'alt+d' }
Ui-ExpectNew 'Profile deleted: Bench Y (user)' 'deleted'
if (Test-Path $printer) { Ui-Fail "the printer's file is still there" }
if (-not (Test-Path $benchX)) { Ui-Fail 'Bench X went too' }
Write-Host 'ok   its file is gone, Bench X stays'
Ui-FocusDialog '^My Library$'
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'My Library printers close' }
Ui-ExpectNew 'My Library closed after changes' 'closed'

Write-Host '--- 3. The design did not change'
Ui-FocusMain
$title = [UiInput]::Title($script:UiWindow)
if ($title.Contains('*')) { Ui-Fail "the design is marked changed: $title" }
if ((Get-FileHash $design).Hash -ne $designSum) { Ui-Fail "the design's file changed" }
Write-Host "ok   the design is unchanged ($title)"

Ui-Finish 'UI Windows My Library test'
