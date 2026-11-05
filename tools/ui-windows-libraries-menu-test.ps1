# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowLibraries.cpp app/files/MainWindowPrint.cpp app/MainWindow.cpp
# check-all sources: app/print app/browser/BrowserController.cpp app/commands/MaterialCommands.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs tools/cli
# Input-driven Windows UI test of Tools > Libraries (mitcad#1250, GitHub
# #132) in the build VM (tools/ui-windows-lib.ps1 says how the input is
# posted), as tools/ui-libraries-menu-test.sh does on Linux. My library is
# the test's own folder (MITCAD_PRINT_LIBRARY_DIR) with a material Shop
# PETG and a printer Bench X. The library entries start by shortcuts of
# the test's.
#   1. With no design open (the empty Untitled window): Tools > Libraries
#      (Alt+T, L) lists, in its groups, Browse Components... (mitcad#1283)
#      | Components in This Design..., Publish to Library... |
#      Material Library..., Printer Library..., Process Library... |
#      Library Sources....
#   2. Material Library...: the Materials manager on the libraries, My
#      library's Shop PETG selected; its density changed, in its file.
#   3. Printer Library...: the Printers manager on My library, Bench X
#      selected; renamed Bench X2 in its file. Process Library...: the
#      Processes manager on My library, which has none. The window's design
#      did not change.
#   4. A design of two boxes, Body1 and Body2 selected, the Material tool:
#      its Material Library... opens the manager on My library while the
#      panel stays open; Duplicate offers My library first; the copy,
#      renamed Bench PETG, is offered by the tool once the manager closes,
#      and OK gives it to both bodies, still selected.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-libraries-menu-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-libraries-menu'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.material_library' = 'F9'; 'tools.printer_library' = 'F8';
                            'tools.process_library' = 'F7'; 'solid.material' = 'F11' } -StepPause 0
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
# The labels' ellipsis and dash, as the log has them (this file is ASCII).
$e = [string][char]0x2026
$dash = [string][char]0x2014

# Ui-ExpectItem path field expected description: a field of a library
# item's file (`material.density`, `profile.name`).
function Ui-ExpectItem([string]$Path, [string]$Field, [string]$Expected, [string]$Description) {
  if (-not (Test-Path $Path)) { Ui-Fail "${Description}: no $Path" }
  $value = Get-Content -Raw $Path | ConvertFrom-Json
  foreach ($part in $Field.Split('.')) { $value = $value.$part }
  # Numbers as numbers: the file may write 1.3 or 1.30.
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

# My library: the material Shop PETG (an item file as Save to writes it)
# and the printer Bench X.
New-Item -ItemType Directory -Force (Join-Path $library 'materials') | Out-Null
$shopPetg = Join-Path $library 'materials\shop_petg.json'
Set-Content -Encoding ascii $shopPetg @'
{
  "format": "mitcad-print-item",
  "version": 1,
  "material": {
    "id": "shop_petg",
    "name": "Shop PETG",
    "density": 1.27,
    "appearance": "plastic_black",
    "filament": {"filament_type": "PETG", "temperature": 240},
    "version": 1
  }
}
'@
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
# The design of part 4: two boxes of the default material.
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'boxes.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Click-InLibrary "text": a click on a logged place of a manager's
# window. The places are logged in the main window's coordinates, as the
# others are, but a click posted to the main window stays in the main
# window's widgets: this one goes to the window Ui-FocusDialog chose, at
# the place moved by the two windows' client origins.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class LibrariesMenuClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InLibrary([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object LibrariesMenuClient+Point
  $dialog = New-Object LibrariesMenuClient+Point
  [LibrariesMenuClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [LibrariesMenuClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

# Picker-Choose "Panel <command> input <id>" "Material" "name": a click on
# the tool's picker button opens its picker window; the name typed in its
# search, Enter chooses the first entry that contains it. (The entry's
# parameter is not $Name: Ui-Step's own $Name would be read, mitcad#1071.)
function Picker-Choose([string]$Button, [string]$Kind, [string]$Entry) {
  Ui-Mark
  Ui-Step "the $Kind picker"             { Ui-ClickLogged $Button }
  Ui-ExpectNew "$Kind picker opened: " "the $Kind picker opened"
  Ui-FocusDialog "^$Kind Picker$"
  Ui-Step "type $Entry"                  { Ui-Type $Entry }
  Ui-ExpectNew "$Kind picker search: $Entry, first $Entry" "$Entry is the first match"
  Ui-Step 'Enter (choose)'               { Ui-Key 'Return' }
  Ui-ExpectNew "$Kind picker chose: $Entry" "$Entry chosen"
  Ui-FocusMain
}

# No design open: the empty Untitled window.
Ui-StartApp 'libraries-menu' @()
$untitled = Ui-WindowTitle

Write-Host '--- 1. Tools > Libraries in its groups'
Ui-FocusMain
Ui-Mark
Ui-Step 'the Tools menu (Alt+T)'         { Ui-Key 'alt+t' }
Ui-ExpectNew 'Tools menu: ' 'the Tools menu opened'
Ui-Step 'Libraries (L)'                  { Ui-Key 'l' }
Ui-ExpectNew 'Libraries menu: ' 'the Libraries menu opened'
$line = Ui-LoggedAfter 'Libraries menu: '
$menu = $line.Substring($line.IndexOf('Libraries menu: ') + 'Libraries menu: '.Length)
$expected = "Browse Components$e | - | Components in This Design$e | Publish to Library$e | - | " +
            "Material Library$e | Printer Library$e | Process Library$e | - | Library Sources$e"
if ($menu -ne $expected) { Ui-Fail "the Libraries menu is '$menu', expected '$expected'" }
Write-Host 'ok   the Libraries menu in its four groups'
Ui-Step 'close the menus (Escape)'       { Ui-Key 'Escape'; Ui-Key 'Escape'; Ui-Key 'Escape' }
Ui-Sync

Write-Host '--- 2. Material Library... without a design: My library, Shop PETG edited'
Ui-FocusMain
Ui-Mark
Ui-Step 'Material Library (F9)'          { Ui-Key 'F9' }
Ui-ExpectNew 'Materials opened without a design' 'the Materials manager opened on the libraries'
Ui-ExpectNew 'Materials loaded: ' 'its list filled when the items were read' 60
Ui-ExpectNew 'Materials groups: My library 1 | Built-in ' 'My library and Built-in, no This design'
Ui-ExpectNew 'Materials on My library: Shop PETG' 'on My library, its first material selected'
Ui-ExpectNew 'Material selected: Shop PETG (user)' 'Shop PETG is selected'
Ui-ExpectNew "Materials editing: Editing: My library $dash every design of yours can use it" 'edited in My library'
Ui-ExpectNew 'Materials density at ' 'Density has its place'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'Density'                        { Click-InLibrary 'Materials density'; Start-Sleep -Milliseconds 200; Ui-Key 'ctrl+a'; Ui-Type '1.3'; Ui-Key 'Return' }
Ui-ExpectNew 'Material Shop PETG (user) density = 1.3' 'Enter saved the density'
Ui-ExpectItem $shopPetg 'material.density' '1.3' "My library's file has the new density"
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'Materials close' }
Ui-ExpectNew 'Materials closed after changes' 'closed'

Write-Host '--- 3. Printer Library... and Process Library... without a design'
Ui-FocusMain
Ui-Mark
Ui-Step 'Printer Library (F8)'           { Ui-Key 'F8' }
Ui-ExpectNew 'Printers opened without a design' 'the Printers manager opened on the libraries'
Ui-ExpectNew 'Printers groups: My library 1 | Built-in ' 'My library and Built-in, no This design' 60
Ui-ExpectNew 'Printers on My library: Bench X' 'on My library, Bench X selected'
Ui-ExpectNew 'Profile selected: printer Bench X (user)' 'Bench X is selected'
Ui-ExpectNew 'Printers name at ' 'its name has its place'
Ui-FocusDialog '^Printers$'
Ui-Mark
Ui-Step 'its name'                       { Click-InLibrary 'Printers name'; Start-Sleep -Milliseconds 200; Ui-Key 'ctrl+a'; Ui-Type 'Bench X2'; Ui-Key 'Return' }
Ui-ExpectNew 'Profile Bench X (user) name = Bench X2' 'renamed in My library'
Ui-ExpectItem $benchX 'profile.name' 'Bench X2' "My library's file has the new name"
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'Printers close' }
Ui-ExpectNew 'Printers closed after changes' 'closed'
Ui-FocusMain
Ui-Mark
Ui-Step 'Process Library (F7)'           { Ui-Key 'F7' }
Ui-ExpectNew 'Processes opened without a design' 'the Processes manager opened on the libraries'
Ui-ExpectNew 'Processes on My library: nothing selected' 'on My library, which has no process' 60
Ui-ExpectNew 'Processes close at ' 'Close has its place'
Ui-FocusDialog '^Processes$'
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'Processes close' }
Ui-ExpectNew 'Processes closed' 'closed'
Ui-FocusMain
if ((Ui-WindowTitle) -ne $untitled) { Ui-Fail "the window's design changed: $(Ui-WindowTitle)" }
Write-Host "ok   the window's design did not change ($untitled)"

Write-Host "--- 4. The Material tool's Material Library...: a material made there is offered, the bodies kept"
Ui-StartApp 'libraries-menu-tool' @('--open', $design)
Ui-ExpectLog 'boxes.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Bodies/Body2 at' 'the browser lists the bodies'
Ui-Mark
Ui-Step 'click Body1'                    { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1' 'Body1 selected'
$at = Ui-LoggedAt 'Browser Root/Bodies/Body2'
Ui-Step 'Ctrl+click Body2'               { Ui-ClickWith 'ctrl' $at[0] $at[1] }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1, Root/Bodies/Body2' 'Body1 and Body2 selected'
Ui-Mark
Ui-Step 'Material (F11)'                 { Ui-Key 'F11' }
Ui-ExpectNew 'Panel Material bodies note: 2 bodies: Steel' 'the panel has both bodies'
Ui-ExpectNew 'Panel Material action library.0 at' 'Material Library... under the picker'
Ui-Mark
Ui-Step 'Material Library...'            { Ui-ClickLogged 'Panel Material action library.0' }
Ui-ExpectNew 'Material tool: Material Library opened' 'the tool opened the manager'
Ui-ExpectNew 'Materials opened' 'the Materials manager opened'
Ui-ExpectNew 'Materials on My library: Shop PETG' 'on My library, Shop PETG selected'
Ui-ExpectNew 'Materials duplicate at ' 'Duplicate under the list'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'Duplicate'                      { Click-InLibrary 'Materials duplicate' }
Ui-ExpectNew 'Materials duplicate menu: My library | This design' 'Duplicate offers My library first'
# The menu opens with nothing active: Down takes its first entry.
Ui-Step 'My library'                     { Ui-Key 'Down'; Ui-Key 'Return' }
Ui-ExpectNew 'Material duplicated: Shop PETG as ' 'a copy made'
$line = Ui-LoggedAfter 'Material duplicated: Shop PETG as '
if ($line -notmatch ' as (\S+) in (\S+)$') { Ui-Fail "no id in: $line" }
if ($Matches[2] -ne 'user') { Ui-Fail "the copy went to '$($Matches[2])', not My library" }
Write-Host 'ok   into My library'
Ui-Sync
Ui-Mark
Ui-Step 'type the name'                  { Ui-Type 'Bench PETG'; Ui-Key 'Return' }
Ui-ExpectNew 'Material Shop PETG (copy) (user) name = Bench PETG' 'Enter renamed it'
Ui-Mark
Ui-Step 'close'                          { Click-InLibrary 'Materials close' }
Ui-ExpectNew 'Materials closed' 'closed'
Ui-ExpectNew 'Material tool: materials read again, ' 'the tool read its materials again'
Ui-ExpectNew ', new: Bench PETG' 'Bench PETG is offered'
Ui-FocusMain
Ui-ExpectLog 'Panel Material input material at' "the picker button's place is logged"
Picker-Choose 'Panel Material input material' 'Material' 'Bench PETG'
Ui-ExpectNew 'Panel Material tile material: Bench PETG (My library)' 'the picker button shows Bench PETG'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Material tool: Bench PETG/' "the tool's log line"
Ui-ExpectNew ' to 2 bodies' 'both bodies kept and given Bench PETG'

Ui-Finish 'UI Windows Tools > Libraries test'
