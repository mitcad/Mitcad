# SPDX-License-Identifier: MIT
# check-all sources: app/print app/framework/NoteRow.cpp app/framework/CommandPanel.cpp app/browser/BrowserController.cpp
# check-all sources: app/browser/LibraryPicker.cpp app/files/MainWindowPrint.cpp core/print-keys core/model/src/print_files tools/cli
# Input-driven Windows UI test of 3D Print's Import from slicer... and its
# Details... (mitcad#654, mitcad#655) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as the last part
# of tools/ui-print-settings-test.sh does on Linux. mitcad-cli writes a
# Bambu Studio project of two boxes of two materials whose filaments are
# slowed for cooling on overhangs from 95%; a design of one box opens.
#   0. The design's process Shop Fine is saved in My library, where another
#      design changed it: 3D Print asks once ("Shop Fine was changed in
#      another design."), Keep this design's settings keeps the design's values (one undo
#      step), and the panel opened again does not ask.
#   1. 3D Print (a shortcut of the test's), its format 3MF, Import
#      from slicer... of the project into the design: the printer and the
#      process came in, the Printer row shows the printer; the line at the
#      panel's end says where the materials went ("2 materials added to
#      Materials"), every setting read (95% too, no "not read").
#   2. Details... beside the line: the whole report, the printer and the
#      process by name, the materials, what was left out; closed with
#      Escape; Escape cancels the panel and the import stays.
#   3. The Printer picker (mitcad#1108; tools/ui-print-picker-test.sh on
#      Linux): two printers named Acme Lab X1 Carbon 0.4 nozzle, the
#      design's and My library's, under their vendor; "X1" typed, Enter
#      chooses the first X1 row, shown without the vendor: the design's
#      printer is set.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-print-settings-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-print-settings'),
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

# The slicer's project, written by mitcad-cli for Bambu Studio.
$project = Join-Path $work 'mount.3mf'
$mountScript = Join-Path $work 'mount.json'
Set-Content -Encoding ascii $mountScript (@'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "printer", "name": "Bench Mini 0.4 nozzle",
   "values": {"bed_shape": [[0, 0], [180, 0], [180, 180], [0, 180]], "max_print_height": 180,
              "nozzle_diameter": 0.4, "printer_settings_id": {"bambu_studio": "Bench Mini 0.4 nozzle"}}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Extra Fine", "values": {"layer_height": 0.08}},
  {"cmd": "set_property", "owner": "C0", "set": "Print Printer", "key": "profile",
   "value": "bench-mini-0-4-nozzle"},
  {"cmd": "set_property", "owner": "C0", "set": "Print Process", "key": "profile", "value": "extra-fine"},
  {"cmd": "create_material", "id": "mount-pla", "name": "Mount PLA", "based_on": "pla",
   "filament": {"overhang_threshold_participating_cooling": "95"}},
  {"cmd": "create_material", "id": "mount-abs", "name": "Mount ABS", "based_on": "abs",
   "filament": {"overhang_threshold_participating_cooling": "95"}},
  {"cmd": "set_component_material", "component": "C0", "material": "mount-pla"},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "mount-abs"},
  {"cmd": "export", "path": "PROJECT", "dialect": "bambu_studio"}
]
'@ -creplace 'PROJECT', ($project -replace '\\', '/'))
$made = & $Cli run $mountScript --save (Join-Path $work 'mount.mitcad')
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $project)) { Ui-Fail "mitcad-cli (the project): $($made -join ' / ')" }

# The design: one box.
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

# The design's process Shop Fine, saved to My library (mitcad#655); then
# another design changes it there: its next version, another layer height.
$savedScript = Join-Path $work 'saved.json'
Set-Content -Encoding ascii $savedScript @'
[
  {"cmd": "create_print_profile", "kind": "process", "name": "Shop Fine", "values": {"layer_height": 0.12},
   "choose": true},
  {"cmd": "save_to_print_library", "library": "user", "kind": "process", "id": "shop-fine"},
  {"cmd": "create_print_profile", "kind": "printer", "name": "Acme Lab X1 Carbon 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}},
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Acme Lab X1 Carbon 0.4 nozzle",
   "values": {"nozzle_diameter": 0.4}}
]
'@
$savedDesign = Join-Path $work 'box-saved.mitcad'
$made = & $Cli run $savedScript --open $design --save $savedDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $savedDesign)) { Ui-Fail "mitcad-cli (Shop Fine into My library): $($made -join ' / ')" }
$design = $savedDesign
$otherScript = Join-Path $work 'other.json'
Set-Content -Encoding ascii $otherScript @'
[
  {"cmd": "edit_print_profile", "library": "user", "id": "shop-fine", "values": {"layer_height": 0.16}}
]
'@
$made = & $Cli run $otherScript --save (Join-Path $work 'other.mitcad')
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli (another design's change): $($made -join ' / ')" }

# 3MF, as the first time's default (mitcad#655), set so that the panel
# starts with it whatever the default.
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @('[print]', 'format=3mf')

Ui-StartApp 'print-settings' @('--open', $design)

Write-Host '--- 0. A saved process another design changed: asked once (mitcad#655)'
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew '3D Print changed elsewhere: Shop Fine' 'asked about the other design''s change'
Ui-FocusDialog '^Changed in Another Design$'
Ui-Step 'Keep (Esc)'                     { Ui-Key 'Escape' }
Ui-ExpectNew '3D Print changed elsewhere: keep' 'the design keeps its values'
Ui-ExpectNew '3D Print process Shop Fine: decline_print_library_update' 'kept, one undo step'
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel the panel (Esc)'         { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed'

Write-Host '--- 1. Import from slicer... in 3D Print: the line under the button'
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew '3D Print printer: Slicer default' 'the panel opened, the design has no printer'
Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
Ui-ExpectNew '; format 3mf, ' 'the panel starts with 3MF'
Ui-ExpectNew 'Panel Send to Slicer action import.0 at ' 'Import from slicer... under the Process row'
$asked = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('changed elsewhere') }
if ($asked) { Ui-Fail "asked again: $($asked -join ' / ')" }
Write-Host 'ok   not asked again'
Ui-Mark
Ui-Step 'Import from slicer...'            { Ui-ClickLogged 'Panel Send to Slicer action import.0' }
Ui-FocusDialog '^Import from Slicer$'
Ui-Step 'type the path'                  { Ui-Key 'ctrl+a'; Ui-Type $project; Start-Sleep -Milliseconds 300; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Imported print settings from mount.3mf: printers: Bench Mini 0.4 nozzle; processes: Extra Fine' `
  'the printer and the process came in'
Ui-ExpectNew '3D Print import: mount.3mf imported' `
  'the line names the file, no names of materials'
$summary = Ui-LogLines | Select-Object -Skip $script:UiMark |
  Where-Object { $_.Contains('3D Print import: mount.3mf imported') } | Select-Object -Last 1
if ("$summary".Contains('not read')) { Ui-Fail "95% was not read: $summary" }
Write-Host 'ok   every setting was read, 95% too'
$imported = Ui-LogLines | Select-Object -Skip $script:UiMark |
  Where-Object { $_.Contains('Imported print settings from mount.3mf') } | Select-Object -Last 1
if ("$imported".Contains('not read')) { Ui-Fail "the import log names values not read: $imported" }
Ui-ExpectNew '3D Print printer: design/bench-mini-0-4-nozzle' 'the Printer row shows the imported printer'
Ui-ExpectNew 'Panel Send to Slicer action import.1 at ' 'Details... beside the line'

Write-Host '--- 2. Details...: the whole report'
Ui-Mark
Ui-Step 'Details...'                       { Ui-ClickLogged 'Panel Send to Slicer action import.1' }
Ui-ExpectNew 'Import details opened: ' 'the whole report opened'
Ui-ExpectNew 'Import details: Imported mount.3mf (Bambu Studio) into this design.' 'its first line'
Ui-ExpectNew 'Import details: Printer: Bench Mini 0.4 nozzle' 'the printer by name'
Ui-ExpectNew 'Import details: Process: Extra Fine' 'the process by name'
Ui-ExpectNew 'Import details: Materials: ' 'the materials by name'
Ui-FocusDialog '^Import Details$'
Ui-Step 'close the report (Esc)'         { Ui-Key 'Escape' }
Ui-ExpectNew 'Import details closed' 'the report closed'
Ui-FocusMain
Ui-Mark
Ui-Step 'cancel the panel (Esc)'         { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed; the import stays'

Write-Host '--- 3. The Printer picker: "X1" typed, Enter chooses the first X1 row (mitcad#1108)'
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer input printer at ' 'the Printer row'
Ui-Mark
Ui-Step 'open Printer'                   { Ui-ClickLogged 'Panel Send to Slicer input printer' }
Ui-ExpectNew 'Printer picker opened: ' 'the picker opened'
# (The built-in printers of the base library, where the VM has it, come
# with their vendors' headings too.)
Ui-ExpectNew 'Printer picker sections: Acme Lab 2 |' 'the two Acme Lab printers under their vendor'
Ui-FocusDialog '^Printer Picker$'
Ui-Mark
Ui-Step 'type X1'                        { Ui-Type 'X1' }
Ui-ExpectNew 'Printer picker search: X1, first X1 Carbon' 'the first X1 row is current'
$found = Ui-LogLines | Select-Object -Skip $script:UiMark |
  Where-Object { $_.Contains('Printer picker search: X1, first ') } | Select-Object -Last 1
if (-not "$found".Contains('0.4 mm nozzle')) { Ui-Fail "the row is not the model and the nozzle: $found" }
Write-Host 'ok   the row shows the model and the nozzle without the vendor'
Ui-Mark
Ui-Step 'choose (Enter)'                 { Ui-Key 'Return' }
Ui-ExpectNew 'Printer picker chose: X1 Carbon' 'Enter chose the first X1 row'
Ui-FocusMain
Ui-ExpectNew '3D Print printer: design/acme-lab-x1-carbon-0-4-nozzle' 'the design''s printer is set'
Ui-ExpectNew 'Panel Send to Slicer input printer shows: X1 Carbon' 'the field shows the model and the nozzle'
Ui-Mark
Ui-Step 'cancel the panel (Esc)'         { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'the panel closed; the choice stays'

Ui-Finish 'UI Windows print settings test'
