# SPDX-License-Identifier: MIT
# check-all sources: app/print app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs tools/cli
# check-all sources: core/model-api/src/print_profiles.rs
# Input-driven Windows UI test of My library without a design (mitcad#704,
# mitcad#1135) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), as tools/ui-my-library-test.sh does on Linux: the Materials
# and Printers managers opened with no design open (the empty Untitled
# window) show My library and Built-in and work on them alone; the
# window's design stays unchanged. My library is the test's own folder
# (MITCAD_PRINT_LIBRARY_DIR), which mitcad-cli gives a printer, Bench X.
#   1. Edit Materials (a shortcut of the test's): the manager's first
#      opening without a design reads the items off the UI thread
#      ("Materials loaded: ..."; mitcad#1018) and lists My library and
#      Built-in; the search finds the built-in PLA Plastic, Duplicate makes
#      PLA Plastic (copy) in My library (its file) at once, renamed Bench
#      PLA, its density 1.3, each written to the file; Delete asks and
#      removes the file.
#   2. Printers... (a shortcut of the test's): Bench X under My library;
#      Duplicate makes Bench X (copy) in My library, renamed Bench Y;
#      Delete removes it, Bench X stays.
#   3. The window's design did not change: no modified mark in the title.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-my-library-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-my-library'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'make.printers' = 'F8'; 'solid.materials' = 'F9' } -StepPause 0
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

# My library: a printer.
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

# Click-InLibrary "text": a click on a logged place of a manager's
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

# No design open: the empty Untitled window.
Ui-StartApp 'my-library' @()

Write-Host '--- 1. Materials without a design: My library and Built-in; a built-in material duplicated, renamed, its density changed, deleted'
Ui-Mark
Ui-Step 'Edit Materials (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Materials opened without a design' 'the Materials manager opened on the libraries'
Ui-ExpectNew 'Materials loaded: ' 'its list filled when the items were read' 60
Ui-ExpectNew 'Materials groups: My library 0 | Built-in ' 'My library and Built-in, no This design'
Ui-ExpectNew 'Materials row: fdm_plastics/pla PLA Plastic (mitcad)' 'PLA is listed, built in'
Ui-ExpectNew 'Materials search at ' 'the search above the list'
# The times as logged, for the record; none is asserted.
foreach ($text in @('Materials timing: ', 'Materials loaded: ')) {
  $line = Ui-LogLines | Where-Object { $_.Contains($text) } | Select-Object -Last 1
  Write-Host "     $($line.Substring($line.IndexOf($text)))"
}
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'search PLA Plastic'             { Click-InLibrary 'Materials search'; Start-Sleep -Milliseconds 200; Ui-Type 'PLA Plastic' }
Ui-ExpectNew 'Materials mitcad item PLA Plastic at ' 'its row has its place'
Ui-Mark
Ui-Step 'PLA Plastic'                    { Click-InLibrary 'Materials mitcad item PLA Plastic' }
Ui-ExpectNew 'Material selected: PLA Plastic (mitcad)' 'PLA is selected'
Ui-ExpectNew 'Materials duplicate at ' 'Duplicate under the list'
Ui-Step 'Duplicate'                      { Click-InLibrary 'Materials duplicate' }
Ui-ExpectNew 'Material duplicated: PLA Plastic as ' 'a copy made, My library the only place to ask for'
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
Ui-ExpectNew 'Materials density at ' 'Density has its place'
Ui-Step 'Density'                        { Click-InLibrary 'Materials density'; Start-Sleep -Milliseconds 200; Ui-Key 'ctrl+a'; Ui-Type '1.3'; Ui-Key 'Return' }
Ui-ExpectNew 'Material Bench PLA (user) density = 1.3' 'Enter saved the density'
Ui-ExpectItem $material 'material.density' '1.3' 'the file has the new density'
Ui-Mark
Ui-Step 'Delete'                         { Click-InLibrary 'Materials delete' }
Ui-ExpectNew 'Materials delete: Delete Bench PLA from My library?' 'Delete asks first, naming the library'
Ui-FocusDialog '^Delete$'
Ui-Step 'Delete (Alt+D)'                 { Ui-Key 'alt+d' }
Ui-ExpectNew 'Material deleted: Bench PLA (user)' 'deleted'
if (Test-Path $material) { Ui-Fail 'the file is still there' }
Write-Host 'ok   its file is gone'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'Materials close' }
Ui-ExpectNew 'Materials closed after changes' 'closed'
Ui-FocusMain

Write-Host '--- 2. Printers without a design: Bench X duplicated, renamed, deleted'
Ui-Mark
Ui-Step 'Printers (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Printers opened without a design' 'the Printers manager opened on the libraries'
Ui-ExpectNew 'Printers groups: My library 1 | Built-in ' 'My library and Built-in, no This design' 60
Ui-ExpectNew 'Printers row: printer/bench-x Bench X (user)' 'Bench X is listed'
Ui-ExpectNew 'Printers user item Bench X at ' 'its row has its place'
Ui-FocusDialog '^Printers$'
Ui-Mark
Ui-Step 'Bench X'                        { Click-InLibrary 'Printers user item Bench X' }
Ui-ExpectNew 'Profile selected: printer Bench X (user)' 'Bench X is selected'
Ui-ExpectNew 'Printers duplicate at ' 'Duplicate under the list'
Ui-Mark
Ui-Step 'Duplicate'                      { Click-InLibrary 'Printers duplicate' }
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
Ui-Step 'Delete'                         { Click-InLibrary 'Printers delete' }
Ui-ExpectNew 'Printers delete: Delete Bench Y from My library?' 'Delete asks first'
Ui-FocusDialog '^Delete$'
Ui-Step 'Delete (Alt+D)'                 { Ui-Key 'alt+d' }
Ui-ExpectNew 'Profile deleted: Bench Y (user)' 'deleted'
if (Test-Path $printer) { Ui-Fail "the printer's file is still there" }
if (-not (Test-Path $benchX)) { Ui-Fail 'Bench X went too' }
Write-Host 'ok   its file is gone, Bench X stays'
Ui-FocusDialog '^Printers$'
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'Printers close' }
Ui-ExpectNew 'Printers closed after changes' 'closed'

Write-Host "--- 3. The window's design did not change"
Ui-FocusMain
$title = [UiInput]::Title($script:UiWindow)
if ($title.Contains('*')) { Ui-Fail "the design is marked changed: $title" }
Write-Host "ok   the design is unchanged ($title)"

Ui-Finish 'UI Windows My library without a design test'
