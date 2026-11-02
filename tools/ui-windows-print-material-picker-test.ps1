# SPDX-License-Identifier: MIT
# check-all sources: app/print app/files/MainWindowPrint.cpp app/browser/LibraryPicker.cpp app/browser/BrowserController.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/Command.hpp app/framework/NoteRow.cpp core/model tools/cli
# Input-driven Windows UI test of Send to Slicer's material rows
# (mitcad#1141) in the build VM (tools/ui-windows-lib.ps1 says how the
# input is posted), as tools/ui-print-material-picker-test.sh does on
# Linux, on a design of three boxes: Body1 without a material of its own,
# Body2 of Shop PETG and Body3 of Shop PLA, and Shop TPU, all three
# materials of the design's with a Bambu Studio filament preset:
#   1. Body1 selected, Send to Slicer (a shortcut of the test's): one row,
#      Material, "No material: choose one", not "Default (Steel)"; no
#      warning names Steel.
#   2. Shop TPU chosen in the row's picker (a window of its own, "Material
#      Picker"): Body1 is of Shop TPU at once, the row shows it, no
#      warning; the panel closed, one Undo takes it back.
#   3. Body2 and Body3 selected: two rows, Filament 1: Shop PETG (1 body)
#      and Filament 2: Shop PLA (1 body); Shop TPU chosen in Filament 1
#      changes Body2 only.
#   4. Edit materials..., the last row of Filament 2's picker, opens the
#      Materials window on Shop PLA.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-print-material-picker-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-print-material-picker'),
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
$editMaterials = 'Edit materials' + [string][char]0x2026
# 3MF, whose print rows are enabled (the design's slicer, Bambu Studio).
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[print]', 'format=3mf')

# Three 20 mm boxes; three materials of the design's, each with a Bambu
# Studio filament preset.
$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'boxes.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [70, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_material", "id": "shop-tpu", "name": "Shop TPU", "based_on": "pla",
   "filament": {"filament_type": "TPU", "filament_settings_id": {"bambu_studio": "Generic TPU"}}},
  {"cmd": "create_material", "id": "shop-petg", "name": "Shop PETG", "based_on": "pla",
   "filament": {"filament_type": "PETG", "filament_settings_id": {"bambu_studio": "Generic PETG"}}},
  {"cmd": "create_material", "id": "shop-pla", "name": "Shop PLA", "based_on": "pla",
   "filament": {"filament_type": "PLA", "filament_settings_id": {"bambu_studio": "Generic PLA"}}},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "shop-petg"},
  {"cmd": "set_body_material", "uid": "F3.b0", "material": "shop-pla"}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli (the design): $($made -join ' / ')" }

# Save-Design: Ctrl+S, the design read back.
function Save-Design {
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Saved ' 'saved'
  return Get-Content -Raw $design | ConvertFrom-Json
}

# No-SteelWarning: no warning since the mark names Steel (the bodies
# without a material are not warned about, mitcad#1141).
function No-SteelWarning {
  $steel = Ui-LogLines | Select-Object -Skip $script:UiMark |
    Where-Object { $_.Contains('names no') -and $_.Contains('Steel') } | Select-Object -Last 1
  if ($steel) { Ui-Fail "a warning names Steel: $steel" }
  Write-Host 'ok   no warning names Steel'
}

# Row-Choose "Panel Send to Slicer input <id>" "text": a click on a material
# row opens its picker window; the text typed in its search, Enter chooses
# the first entry that contains it, which is `$Entry`.
function Row-Choose([string]$Row, [string]$Typed, [string]$Entry) {
  Ui-Mark
  Ui-Step 'open the material row'        { Ui-ClickLogged $Row }
  Ui-ExpectNew 'Material picker opened: ' 'its picker opened'
  Ui-FocusDialog '^Material Picker$'
  Ui-Step "type $Typed"                  { Ui-Type $Typed }
  Ui-ExpectNew "Material picker search: $Typed, first $Entry" "$Entry is the first match"
  Ui-Step 'Enter (choose)'               { Ui-Key 'Return' }
  Ui-ExpectNew "Material picker chose: $Entry" "$Entry chosen"
}

Ui-StartApp 'print-material-picker' @('--open', $design)
Ui-ExpectLog 'boxes.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Root/Bodies/Body3 at' 'the browser lists the bodies'

Write-Host '--- 1. Body1 without a material: "No material: choose one"'
Ui-Mark
Ui-Step 'click Body1'                    { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body1' 'Body1 selected'
Ui-Mark
Ui-Step 'Send to Slicer (F8)'            { Ui-Key 'F8' }
Ui-ExpectNew '3D Print materials: Material: No material: choose one' 'one row, Body1 without a material'
Ui-ExpectNew 'Panel Send to Slicer input material_1 shows: No material: choose one' `
  'the field says so, not Default (Steel)'
Ui-ExpectNew 'Panel Send to Slicer input material_1 at ' 'the Material row has its place'
No-SteelWarning

Write-Host '--- 2. Shop TPU chosen in the row: Body1''s material at once, one undo step'
Row-Choose 'Panel Send to Slicer input material_1' 'Shop TPU' 'Shop TPU'
Ui-FocusMain
Ui-ExpectNew '3D Print material: Shop TPU for Body1' 'given to Body1 at once'
Ui-ExpectNew 'Material shop-tpu set for 1 body(ies), appearance replace' 'as the Material tool logs it'
Ui-ExpectNew '3D Print materials: Material: Shop TPU' 'the row shows Shop TPU'
Ui-ExpectNew 'Panel Send to Slicer input material_1 shows: Shop TPU' 'the field shows Shop TPU'
No-SteelWarning
Ui-Mark
Ui-Step 'close the panel (Esc)'          { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed; the material stays'
$saved = Save-Design
if ($saved.bodies[0].material -ne 'shop-tpu') { Ui-Fail "Body1 is of '$($saved.bodies[0].material)', not Shop TPU" }
Write-Host 'ok   Body1 is of Shop TPU'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Set Material of Body1' 'one undo step'
$saved = Save-Design
if ($saved.bodies[0].material) { Ui-Fail "Body1 is still of '$($saved.bodies[0].material)'" }
Write-Host 'ok   Body1 has no material of its own again'

Write-Host '--- 3. Body2 (Shop PETG) and Body3 (Shop PLA): two rows, one changed alone'
Ui-Step 'Escape (clear the selection)'   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'click Body2'                    { Ui-ClickLogged 'Browser Root/Bodies/Body2' }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body2' 'Body2 selected'
$at = Ui-LoggedAt 'Browser Root/Bodies/Body3'
Ui-Step 'Ctrl+click Body3'               { Ui-ClickWith 'ctrl' $at[0] $at[1] }
Ui-ExpectNew 'Browser selected: Root/Bodies/Body2, Root/Bodies/Body3' 'Body2 and Body3 selected'
Ui-Mark
Ui-Step 'Send to Slicer (F8)'            { Ui-Key 'F8' }
Ui-ExpectNew '3D Print materials: Filament 1: Shop PETG (1 body) | Filament 2: Shop PLA (1 body)' 'a row per filament'
Ui-ExpectNew 'Panel Send to Slicer label material_1: Filament 1' 'the first row is Filament 1'
Ui-ExpectNew 'Panel Send to Slicer label material_2: Filament 2' 'the second Filament 2'
Ui-ExpectNew 'Panel Send to Slicer input material_1 at ' 'Filament 1 has its place'
Row-Choose 'Panel Send to Slicer input material_1' 'Shop TPU' 'Shop TPU'
Ui-FocusMain
Ui-ExpectNew '3D Print material: Shop TPU for Body2' 'given to Body2 only'
Ui-ExpectNew '3D Print materials: Filament 1: Shop TPU (1 body) | Filament 2: Shop PLA (1 body)' `
  'Filament 2 stays Shop PLA'

Write-Host '--- 4. Edit materials... of Filament 2: the Materials window on Shop PLA'
Ui-ExpectLog 'Panel Send to Slicer input material_2 at ' 'Filament 2 has its place'
Row-Choose 'Panel Send to Slicer input material_2' 'Edit materials' $editMaterials
Ui-ExpectNew "3D Print: Edit materials$([char]0x2026) of Shop PLA (Body3)" 'for Filament 2''s material'
Ui-ExpectNew 'Materials opened for bodies F3.b0' 'Materials opened for Body3'
Ui-ExpectNew 'Material selected: Shop PLA (design)' 'on Shop PLA'
# The Materials window is on top and takes the keys: Escape closes it
# first (as Edit printers and processes... does, mitcad#1134), then the
# panel's own Escape (which then reaching the panel shows it closed).
Ui-Step 'close Materials (Escape)'       { Ui-FocusDialog '^Materials$'; Ui-Key 'Escape'; Ui-FocusMain }
Ui-Mark
Ui-Step 'close the panel (Esc)'          { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed'
$saved = Save-Design
if ($saved.bodies[1].material -ne 'shop-tpu' -or $saved.bodies[2].material -ne 'shop-pla') {
  Ui-Fail "Body2 and Body3 are of '$($saved.bodies[1].material)' and '$($saved.bodies[2].material)'"
}
Write-Host 'ok   Body2 is of Shop TPU, Body3 still of Shop PLA'

Ui-Finish 'UI Windows print material picker test'
