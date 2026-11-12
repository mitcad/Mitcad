# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MaterialCommands.cpp app/commands/MaterialBinding.cpp app/browser/LibraryPicker.cpp app/browser/BrowserController.cpp app/browser/BrowserMaterialMenu.cpp app/framework/CommandPanel.cpp app/files/MainWindowPrint.cpp app/print tools/cli
# Input-driven Windows UI test of the Material and Appearance tools
# (mitcad#1059) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted), as tools/ui-material-tool-test.sh does on Linux. The tools
# start by shortcuts of the test's; the pickers are windows of their own
# ("Material Picker"), which take the posted keys as a dialog does.
#   1. Body1 and Body2 selected in the Browser, the Material tool: the
#      panel notes "2 bodies: Steel"; Shop PETG of My library chosen in the
#      picker; OK gives both bodies a copy of it, one undo step; Ctrl+Z
#      restores both.
#   2. Body3, whose material follows Color (written by mitcad-cli as the
#      windows of mitcad#861 wrote it), selected: the tool shows the binding,
#      its rows Black (now) and Red. (Choosing the rows' values needs the
#      drop-downs' lists, which close at once in this session: the Linux
#      test binds a material and switches the option.)
#   3. The Appearance tool on Body2: Paint - Red in its picker; then Body2's
#      top face in Paint - Black through the Faces input.
#   4. The root's Material > More... (its context menu), the Material tool: Acme
#      PETG as its default.
#   5. Edit Materials opens the Materials window without Assign.
#   6. Edit Materials... last in the Material tool's picker (mitcad#1415):
#      Body2 selected, Shop PETG chosen, Edit Materials... (Edit typed,
#      Enter) opens the Materials manager on Shop PETG while the panel
#      stays open; its copy in My library, renamed Bench PETG, is offered
#      once the manager closes; OK gives it to Body2 in one undo step.
#   7. The same with the Appearance tool's Edit Appearances...: the window
#      on Body2's Paint - Red, New there renamed Shop Red, offered once the
#      window closes, given to Body2 in one undo step.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-material-tool-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-material-tool'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.material' = 'F8'; 'solid.appearance' = 'F9'; 'solid.materials' = 'F7' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# My library with one material of its own.
$library = Join-Path $work 'print-library'
$env:MITCAD_PRINT_LIBRARY_DIR = $library
New-Item -ItemType Directory -Force (Join-Path $library 'materials') | Out-Null
Set-Content -Encoding ascii (Join-Path $library 'materials\shop_petg.json') @'
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

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'boxes.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [55, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "acme_petg", "name": "Acme PETG", "density": 1.27,
   "appearance": "plastic_orange", "filament": {"filament_type": "PETG", "temperature": 240}},
  {"cmd": "add_parameter", "name": "Color", "expression": "'Black'", "values": ["'Black'", "'Red'"]},
  {"cmd": "set_body_material", "uid": "F3.b0",
   "expression": "switch(Color; 'Black'; 'acme_petg'; 'Red'; 'pla')"}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# Save-Design: Ctrl+S, the design read back.
function Save-Design {
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Saved ' 'saved'
  return Get-Content -Raw $design | ConvertFrom-Json
}

# Picker-Choose "Panel <command> input <id>" "Material" "name": a click on
# the tool's picker button opens its picker window; the name typed in its
# search, Enter chooses the first entry that contains it.
# The entry's parameter is not $Name: a step's script block runs inside
# Ui-Step, whose own $Name (the step's description, "type Shop PETG") it
# would read, and that text was typed (mitcad#1071).
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

# Picker-Edit "Panel <command> input <id>" "Material" "Edit Materials...":
# the picker's last entry (mitcad#1415) found by typing Edit and chosen
# with Enter, as the keyboard reaches the other entries; it opens a window.
function Picker-Edit([string]$Button, [string]$Kind, [string]$Entry) {
  Ui-Mark
  Ui-Step "the $Kind picker"             { Ui-ClickLogged $Button }
  Ui-ExpectNew "$Kind picker opened: " "the $Kind picker opened"
  Ui-FocusDialog "^$Kind Picker$"
  Ui-Step 'type Edit'                    { Ui-Type 'Edit' }
  Ui-ExpectNew "$Kind picker search: Edit, first $Entry" "typing Edit finds the last entry"
  Ui-Step 'Enter (choose)'               { Ui-Key 'Return' }
  Ui-ExpectNew "$Kind picker chose: $Entry" 'the last entry chosen'
}

# Ui-LoggedAfter "text": the last line since the mark with the text.
function Ui-LoggedAfter([string]$Text) {
  return Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains($Text) } | Select-Object -Last 1
}

# Click-InDialog "text": a click on a logged place of a window of its own
# (the Materials or Appearances window). The places are logged in the main
# window's coordinates, as the others are, but a click posted to the main
# window stays in the main window's widgets: this one goes to the window
# Ui-FocusDialog chose, at the place moved by the two windows' client
# origins (as tools/ui-windows-libraries-menu-test.ps1 clicks).
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class MaterialToolClient {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
}
'@
function Click-InDialog([string]$Text) {
  $at = Ui-LoggedAt $Text
  $main = New-Object MaterialToolClient+Point
  $dialog = New-Object MaterialToolClient+Point
  [MaterialToolClient]::ClientToScreen($script:UiWindow, [ref]$main) | Out-Null
  [MaterialToolClient]::ClientToScreen($script:UiTarget, [ref]$dialog) | Out-Null
  $x = $at[0] + $main.X - $dialog.X
  $y = $at[1] + $main.Y - $dialog.Y
  $window = $script:UiWindow
  $script:UiWindow = $script:UiTarget
  try { Ui-Click $x $y } finally { $script:UiWindow = $window }
}

Ui-StartApp 'material-tool' @('--open', $design)
Ui-ExpectLog 'boxes.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Bodies/Body3 at' 'the browser lists the bodies'

Write-Host '--- 1. A library material for two bodies, one undo step'
Ui-Mark
Ui-Step 'click Body1'                    { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1' 'Body1 selected'
$at = Ui-LoggedAt 'Browser Root/Bodies/Body2'
Ui-Step 'Ctrl+click Body2'               { Ui-ClickWith 'ctrl' $at[0] $at[1] }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1, Root/Bodies/Body2' 'Body1 and Body2 selected'
Ui-Mark
Ui-Step 'Material (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Material bodies note: 2 bodies: Steel' 'the panel notes what the bodies are made of'
Ui-ExpectNew 'Panel Material input material at' 'the picker button is logged'
Picker-Choose 'Panel Material input material' 'Material' 'Shop PETG'
Ui-ExpectNew 'Panel Material tile material: Shop PETG (My library)' 'the picker button shows Shop PETG'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Material tool: Shop PETG/' 'the tool''s log line'
$saved = Save-Design
$copy = @($saved.materials | Where-Object { $_.name -eq 'Shop PETG' })
if ($copy.Count -ne 1) { Ui-Fail 'the design has no copy of Shop PETG' }
if ($saved.bodies[0].material -ne $copy[0].id -or $saved.bodies[1].material -ne $copy[0].id) {
  Ui-Fail "Body1 and Body2 are not Shop PETG: $($saved.bodies[0].material), $($saved.bodies[1].material)"
}
Write-Host 'ok   both bodies are the design''s copy of Shop PETG'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Material' 'one undo step'
$saved = Save-Design
if ($null -ne $saved.bodies[0].material -or $null -ne $saved.bodies[1].material) {
  Ui-Fail 'Ctrl+Z did not give both bodies their default back'
}
Write-Host 'ok   both have their default again'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- 2. A binding made earlier shows in the tool'
Ui-Mark
Ui-Step 'click Body3'                    { Ui-ClickLogged 'Browser Root/Bodies/Body3' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body3' 'Body3 selected'
Ui-Step 'Material (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew "Material tool: the targets follow switch(Color; 'Black'; 'acme_petg'; 'Red'; 'pla')" `
  'the tool reads Body3''s binding'
Ui-ExpectNew 'Panel Material label row0: Black (now)' 'the row Black, the value now'
Ui-ExpectNew 'Panel Material label row1: Red' 'the row Red'
Ui-Step 'Cancel (Escape)'                { Ui-Key 'Escape' }
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- 3. The Appearance tool: a body, then a face'
Ui-Mark
Ui-Step 'click Body2'                    { Ui-ClickLogged 'Browser Root/Bodies/Body2' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body2' 'Body2 selected'
Ui-Step 'Appearance (F9)'                { Ui-Key 'F9' }
Ui-ExpectNew 'Panel Appearance input value at' 'the picker button is logged'
Picker-Choose 'Panel Appearance input value' 'Appearance' 'Paint - Red'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Appearance paint_red for 1 body(ies)' 'Body2 is red'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'Appearance (F9)'                { Ui-Key 'F9' }
Ui-ExpectNew 'Panel Appearance input faces at' 'the Faces input is logged'
Ui-Step 'the Faces input'                { Ui-ClickLogged 'Panel Appearance input faces' }
Ui-Step 'pick Body2''s top face'         { Ui-ClickPick 'face F2.b0/F2:top' }
Picker-Choose 'Panel Appearance input value' 'Appearance' 'Paint - Black'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Appearance paint_black for 0 body(ies), 1 face(s)' 'the top face is black'
$saved = Save-Design
if ($saved.bodies[1].appearance -ne 'paint_red' -or @($saved.bodies[1].face_appearances).Count -ne 1) {
  Ui-Fail 'Body2 is not red with a face of its own'
}
Write-Host 'ok   Body2 red, its top face its own'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- 4. The root''s default material'
# The root's row selects nothing (the root is everything: no component
# for the tool's Bodies), so the root's Material > More... starts the tool
# for it, as the Linux test does (mitcad#1071; a click and F8 left Bodies
# empty and logged no note). The menus open with nothing active, a
# submenu with its first entry; the keys choose by the logged places.
function Logged-Entries([string]$Menu) {
  return @("$(Ui-LastMatch "^Context menu${Menu}: (.*)$")" -split ' \| ')
}
Ui-FocusMain
Ui-Mark
Ui-Step 'right-click Root'               { Ui-ClickLogged 'Browser Root' 3 }
Ui-ExpectNew 'Context menu: ' 'the root''s menu opened'
$rootEntries = Logged-Entries ''
$materialAt = [array]::IndexOf($rootEntries, 'Material')
if ($materialAt -lt 0) { Ui-Fail "no Material in the root's menu: $($rootEntries -join ' | ')" }
Ui-Step 'Material'                       {
  for ($down = 0; $down -le $materialAt; $down++) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
Ui-ExpectNew 'Context menu Material: ' 'the Material submenu opened'
$more = 'More' + [string][char]0x2026
$submenuEntries = Logged-Entries ' Material'
$moreAt = [array]::IndexOf($submenuEntries, $more)
if ($moreAt -lt 0) { Ui-Fail "no More... in Material: $($submenuEntries -join ' | ')" }
Ui-Step 'More...'                        {
  for ($down = 0; $down -lt $moreAt; $down++) { Ui-Key 'Down' }
  Ui-Key 'Return'
}
Ui-ExpectNew 'Material tool for component C0' 'the Material tool for the root'
Ui-ExpectNew 'Panel Material bodies note: 1 component: Default' 'the panel notes the component'
Picker-Choose 'Panel Material input material' 'Material' 'Acme PETG'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Default material of Root: acme_petg' 'the root''s default is Acme PETG'
$saved = Save-Design
if (@($saved.component_materials | Where-Object { $_.component -eq 'C0' -and $_.material -eq 'acme_petg' }).Count -ne 1) {
  Ui-Fail 'the file does not keep the root''s default'
}
Write-Host 'ok   the file keeps the root''s default'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- 5. The Materials window manages only'
Ui-Mark
Ui-Step 'Edit Materials (F7)'            { Ui-Key 'F7' }
Ui-ExpectNew 'Materials opened' 'the Materials window opened'
Ui-ExpectNew 'Materials new at' 'its New button is logged'
if (Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.StartsWith('Materials assign') }) {
  Ui-Fail 'the Materials window has an Assign button'
}
Write-Host 'ok   no Assign'
Ui-FocusDialog '^Materials$'
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-FocusMain

# The pickers' last entries (mitcad#1415). The log has the ellipsis; this
# file is read as ANSI, so it is made of its code point.
$ellipsis = [string][char]0x2026

Write-Host '--- 6. Edit Materials... at the end of the picker: a material made there is offered'
Ui-Mark
Ui-Step 'click Body2'                    { Ui-ClickLogged 'Browser Root/Bodies/Body2' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body2' 'Body2 selected'
Ui-Mark
Ui-Step 'Material (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Material input material at' 'the picker button is logged'
Picker-Choose 'Panel Material input material' 'Material' 'Shop PETG'
Picker-Edit 'Panel Material input material' 'Material' ('Edit Materials' + $ellipsis)
Ui-ExpectNew ("Material tool: Edit Materials$ellipsis on Shop PETG") 'the tool opens the manager on its material'
Ui-ExpectNew 'Materials opened' 'the Materials manager opened'
Ui-ExpectNew 'Material selected: Shop PETG (user)' 'on Shop PETG'
Ui-ExpectNew 'Materials duplicate at ' 'Duplicate under the list'
Ui-FocusDialog '^Materials$'
Ui-Mark
Ui-Step 'Duplicate'                      { Click-InDialog 'Materials duplicate' }
Ui-ExpectNew 'Materials duplicate menu: This design | My library' 'Duplicate offers This design first while a design is open'
# The menu opens with nothing active: Down takes This design, Down again My library.
Ui-Step 'My library'                     { Ui-Key 'Down'; Ui-Key 'Down'; Ui-Key 'Return' }
Ui-ExpectNew 'Material duplicated: Shop PETG as ' 'a copy made while the tool''s panel is open'
$line = Ui-LoggedAfter 'Material duplicated: Shop PETG as '
if ($line -notmatch ' as (\S+) in (\S+)$') { Ui-Fail "no id in: $line" }
if ($Matches[2] -ne 'user') { Ui-Fail "the copy went to '$($Matches[2])', not My library" }
Write-Host 'ok   into My library'
Ui-Sync
Ui-Mark
Ui-Step 'type the name'                  { Ui-Type 'Bench PETG'; Ui-Key 'Return' }
Ui-ExpectNew 'Material Shop PETG (copy) (user) name = Bench PETG' 'Enter renamed it'
Ui-Mark
Ui-Step 'close (Escape)'                 { Ui-Key 'Escape' }
Ui-ExpectNew 'Materials closed' 'closed'
Ui-ExpectNew 'Material tool: materials read again, ' 'the tool read its materials again'
Ui-ExpectNew ', new: Bench PETG' 'Bench PETG is offered'
Ui-FocusMain
Picker-Choose 'Panel Material input material' 'Material' 'Bench PETG'
Ui-ExpectNew 'Panel Material tile material: Bench PETG (My library)' 'the picker button shows Bench PETG'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Material tool: Bench PETG/' 'the tool''s log line'
Ui-ExpectNew ' to 1 bodies' 'Body2, still selected, gets Bench PETG'
$saved = Save-Design
$copy = @($saved.materials | Where-Object { $_.name -eq 'Bench PETG' })
if ($copy.Count -ne 1 -or $saved.bodies[1].material -ne $copy[0].id) {
  Ui-Fail "Body2 is not the design's copy of Bench PETG: $($saved.bodies[1].material)"
}
Write-Host 'ok   Body2 is the design''s copy of Bench PETG'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Set Material of Body2' 'one undo step'
$saved = Save-Design
if ($null -ne $saved.bodies[1].material -or @($saved.materials | Where-Object { $_.name -eq 'Bench PETG' }).Count -ne 0) {
  Ui-Fail 'Ctrl+Z did not give Body2 its default back and take the copy away'
}
Write-Host 'ok   Body2 has its default again, the copy gone with it'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

Write-Host '--- 7. Edit Appearances... at the end of the picker: an appearance made there is offered'
Ui-Mark
Ui-Step 'click Body2'                    { Ui-ClickLogged 'Browser Root/Bodies/Body2' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body2' 'Body2 selected'
Ui-Mark
Ui-Step 'Appearance (F9)'                { Ui-Key 'F9' }
Ui-ExpectNew 'Panel Appearance tile value: Paint - Red (Library)' 'the tool starts with Body2''s Paint - Red'
Ui-ExpectNew 'Panel Appearance input value at' 'the picker button is logged'
Picker-Edit 'Panel Appearance input value' 'Appearance' ('Edit Appearances' + $ellipsis)
Ui-ExpectNew ("Appearance tool: Edit Appearances$ellipsis on Paint - Red") 'the tool opens the window on its appearance'
Ui-ExpectNew 'Appearances dialog opened' 'the Appearances window opened'
Ui-ExpectNew 'Appearances on paint_red' 'on Paint - Red'
Ui-ExpectNew 'Appearances new at ' 'its New button is logged'
Ui-FocusDialog '^Appearances$'
Ui-Mark
Ui-Step 'New'                            { Click-InDialog 'Appearances new' }
Ui-ExpectNew ' (Paint - Red Copy) from paint_red' 'a copy of Paint - Red while the tool''s panel is open'
$line = Ui-LoggedAfter ' (Paint - Red Copy) from paint_red'
if ($line -notmatch 'Created appearance (\S+) \(') { Ui-Fail "no id in: $line" }
$look = $Matches[1]
Ui-Sync
Ui-Step 'type the name'                  { Ui-Type 'Shop Red'; Ui-Key 'Return' }
Ui-ExpectNew "Appearance $look name = Shop Red" 'Enter renamed it'
Ui-Mark
Ui-Step 'close'                          { Click-InDialog 'Appearances close' }
Ui-ExpectNew 'Appearances closed' 'closed'
Ui-ExpectNew 'Appearance tool: appearances read again, ' 'the tool read its appearances again'
Ui-ExpectNew ', new: Shop Red' 'Shop Red is offered'
Ui-FocusMain
Picker-Choose 'Panel Appearance input value' 'Appearance' 'Shop Red'
Ui-ExpectNew 'Panel Appearance tile value: Shop Red (This design)' 'the picker button shows Shop Red'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew "Appearance $look for 1 body(ies)" 'Body2, still selected, gets Shop Red'
$saved = Save-Design
if ($saved.bodies[1].appearance -ne $look) { Ui-Fail "Body2's appearance is $($saved.bodies[1].appearance), not $look" }
Write-Host 'ok   the file keeps Body2''s new appearance'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Set Appearance of Body2' 'one undo step'
$saved = Save-Design
if ($saved.bodies[1].appearance -ne 'paint_red') { Ui-Fail "Ctrl+Z left Body2 $($saved.bodies[1].appearance)" }
if (@($saved.appearances | Where-Object { $_.name -eq 'Shop Red' }).Count -ne 1) { Ui-Fail 'the new appearance is gone' }
Write-Host 'ok   Body2 is Paint - Red again, Shop Red stays in the design'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }

if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }
Ui-Finish 'Windows UI Material and Appearance tools'
