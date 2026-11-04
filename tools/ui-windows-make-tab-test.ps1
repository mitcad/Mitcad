# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/MainWindowMake.cpp app/print/MakeBodyCard.cpp app/browser/BrowserPanel.cpp
# check-all sources: core/model-api/src/body_notes.rs core/model-api/src/cut_sheets.rs tools/cli
# check-all sources: core/model-api/src/annotations.rs app/MainWindowMeasurements.cpp app/view/MeasurementOverlay.cpp
# check-all sources: app/print/PrintValueChoices.cpp app/print/PrintValuePicker.cpp app/MainWindowSelectionCreate.cpp
# check-all sources: app/framework/PickerComboBox.cpp app/browser/LibraryPicker.cpp app/print/ProfilePickerRows.cpp
# Input-driven Windows UI test of the Make tab (mitcad#1049) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-make-tab-test.sh on Linux, on its design: a sheet-metal plate
# with its flat pattern (Body1) and a 40 x 30 x 10 block with a
# counterbored hole (Body2) that sets Walls 5 of its own; the design prints
# with the printer Bench and the process Standard.
#   1. The tabs Solid, Build and Make; the Make tab's groups Materials,
#      Annotate, 3D Print and Laser, their menus; no CNC group. The Browser
#      marks Body2 on the Make tab.
#   2. The moved commands from the Make tab's buttons: Edit Materials...,
#      Printers... and Processes... (mitcad#1135), Print Analysis, Send to Slicer and
#      Export Flat Pattern... (its DXF written); Export Face as DXF... on
#      Body2's top face.
#   3. The Annotate group places each kind: Dimension and Note from their
#      buttons (the note previewed before Add, then added where its preview
#      was, mitcad#1097), Datum Label, Feature Control Frame, Hole Callout,
#      Material and Process Note by shortcuts of the test's (their menu entries are
#      in the Linux test: the input goes to the main window's places only).
#   4. Body2's card on the Make tab: Walls 5 set on the body, typed 3 there,
#      one Undo back to 5; the Make section inside the selection card, its
#      frame whole, the same margin on both sides (mitcad#1178: the design
#      has a process of a long name, which made it wider). Body1's card
#      (mitcad#1178): no Create row, no print values of its own; Add Print
#      Value... and "Walls" typed in its picker add the process's 2 walls
#      to Body1 with the body's icon, its field focused; 4 typed there is
#      set; its remove button clears it; one Undo puts 4 back. None on the
#      Solid tab, nor markers.
#   4c. The card's Printer and Process pickers (mitcad#1239), on a second
#      design with the printer Bench but none chosen, and My library with
#      the processes Coarse @Alpha and Fine @Bench: the rows Printer,
#      Process, Material; the Printer picker lists Bench, "Bench" typed
#      and Enter make it the design's printer (the root's) and Undo gives
#      Slicer default again; chosen again, the Process picker lists Bench's
#      library process first, and choosing it sets it on the body (one undo
#      step); the Printer picker's last row, Printers..., opens the Printers
#      manager on Bench.
#   5. The Floating layout: the Make tab and Dimension from it.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-make-tab-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-make-tab'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{
  'make.datum_label' = 'Shift+F3'; 'make.frame' = 'Shift+F4'; 'make.hole_callout' = 'Shift+F5'
  'make.material_note' = 'Shift+F6'; 'make.process_note' = 'Shift+F7' } -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$workLogged = $work -replace '\\', '/'

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$modelJson = Join-Path $work 'make.json'
$design = Join-Path $work 'make.mitcad'
Set-Content -Encoding ascii $modelJson @'
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":1,"bend_radius":2,"allowance":{"type":"k_factor","k":0.4},"reference_side":"front","bend_relief":{"type":"rectangular","width":1,"depth":1},"corner_relief":{"type":"round","width":2,"depth":2}}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F2","corner":[-40,0],"width":30,"height":20},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F2","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}]}},
  {"cmd":"set_flat_pattern","body":"F3.b0"},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F4","corner":[0,0],"width":40,"height":30},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F4","region":"r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent":{"type":"distance","distance":10},"operation":"new_body"}},
  {"cmd":"add_feature","def":{"type":"hole",
    "placement":{"type":"face","body":"F5.b0","face":"F5:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})","points":[[20,15,10]]},
    "diameter":6.6,"extent":{"type":"through_all"},
    "kind":{"type":"counterbore","diameter":11,"depth":4}}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"process","name":"Standard","values":{"layer_height":0.2,"perimeters":2}},
  {"cmd":"set_property","owner":"C0","set":"Print Printer","key":"profile","value":"bench"},
  {"cmd":"set_property","owner":"C0","set":"Print Process","key":"profile","value":"standard"},
  {"cmd":"set_property","owner":"F5.b0","set":"Print Process","key":"perimeters","value":5},
  {"cmd":"create_print_profile","kind":"process","name":"Fine detail 0.12 mm layers with extra walls and a slow outer wall","values":{"layer_height":0.12}}
]
'@
$output = & $cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }
# The card's pickers (mitcad#1239): a box with the printer Bench in the
# design, none chosen, no process.
$pickersJson = Join-Path $work 'pickers.json'
$pickers = Join-Path $work 'pickers.mitcad'
Set-Content -Encoding ascii $pickersJson @'
[
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[-10,-10],"length":20,"width":20,"height":20,"operation":"new_body"}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench","values":{"nozzle_diameter":0.4}}
]
'@
$output = & $cli run $pickersJson --save $pickers 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli (pickers): $($output -join ' / ')" }
# My library in the test's own folder: a process for Bench and one for
# another printer, by their names' @.
$library = Join-Path $work 'print-library'
$env:MITCAD_PRINT_LIBRARY_DIR = $library
New-Item -ItemType Directory -Force (Join-Path $library 'print\processes') | Out-Null
foreach ($process in @(@('fine-bench', 'Fine @Bench', '0.12'), @('coarse-alpha', 'Coarse @Alpha', '0.28'))) {
  Set-Content -Encoding ascii (Join-Path $library "print\processes\$($process[0]).json") `
    ('{"format": "mitcad-print-item", "version": 1, "profile": {"id": "' + $process[0] + '", "name": "' + $process[1] +
     '", "kind": "process", "values": {"layer_height": ' + $process[2] + '}, "version": 1}}')
}

$ellipsis = [string][char]0x2026
# Group-Menu "TAB/GROUP" "entries": the group's menu opened, its entries
# (an ellipsis as three dots) checked, closed.
function Group-Menu([string]$Group, [string]$Entries) {
  Ui-Mark
  Ui-Step "open the $Group menu"           { Ui-ClickLogged "Ribbon $Group" }
  Ui-ExpectNew "Ribbon menu ${Group}: " "the $Group menu opened"
  $shown = "$(Ui-LastMatch "^Ribbon menu ${Group}: (.*)$")".Replace($ellipsis, '...')
  if ($shown -ne $Entries) { Ui-Fail "the $Group menu is '$shown', not '$Entries'" }
  Write-Host "ok   the $Group menu: $Entries"
  Ui-Step 'close it (Escape)'              { Ui-Key 'Escape' }
}
# Save-Path "dialog title" path: a path typed into the file dialog.
function Save-Path([string]$Title, [string]$Path) {
  Ui-Step "type $Path" {
    Ui-FocusDialog $Title
    Ui-Key 'ctrl+a'
    Ui-Type $Path
    Start-Sleep -Milliseconds 300
    Ui-Key 'Return'
    Ui-FocusMain
  }
}
# Annotate "key or button" "command" "pick": an Annotate panel started, its
# one pick made, added (Enter).
function Annotate([string]$Start, [string]$Command, [string]$Pick) {
  Ui-Mark
  if ($Start.StartsWith('Ribbon ')) {
    Ui-Step $Command                       { Ui-ClickLogged $Start }
  } else {
    Ui-Step "$Command ($Start)"            { Ui-Key $Start }
  }
  Ui-ExpectNew "Command $Command started" "$Command started"
  Ui-Step "pick $Pick"                     { Ui-ClickPick $Pick }
}

# Section-Fits "Make": the selection card's section as last logged
# (mitcad#1178): inside the card and what of it shows, its right frame
# whole, with the same margin on both sides unless the page scrolls.
function Section-Fits([string]$Name) {
  $line = "$(Ui-LastLineWith "Selection card section ${Name}: ")"
  $m = [regex]::Match($line, ': (-?\d+)\.\.(-?\d+) of card (-?\d+)\.\.(-?\d+), visible to (-?\d+)')
  if (-not $m.Success) { Ui-Fail "no place logged for the $Name section ($line)" }
  $l = [int]$m.Groups[1].Value; $r = [int]$m.Groups[2].Value
  $cl = [int]$m.Groups[3].Value; $cr = [int]$m.Groups[4].Value; $vr = [int]$m.Groups[5].Value
  if ($l -lt $cl -or $r -gt $cr -or $r -gt $vr) { Ui-Fail "the $Name section runs past the card: $line" }
  if (-not $line.EndsWith(', scrolled') -and [Math]::Abs(($l - $cl) - ($cr - $r)) -gt 1) {
    Ui-Fail "the $Name section's margins differ (left $($l - $cl), right $($cr - $r)): $line"
  }
  Write-Host "ok   the $Name section inside the card ($l..$r of $cl..$cr, visible to $vr)"
}

$top = 'face F5.b0/F5:end(r{c1'
$cbore = 'face F5.b0/F6:hole0.cbore_wall'

Ui-StartApp 'make-tab' @('--open', $design, '--no-native-dialogs')
Ui-ExpectLog "Opened $workLogged/make.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                         { Ui-Key 'F6' }
Ui-ExpectLog 'Browser Make markers: none' 'no markers on the Solid tab'

Write-Host '--- 1. The Make tab and its groups'
Ui-ExpectLog 'Ribbon tab MAKE at' 'the Make tab after Solid and Build'
Ui-Mark
Ui-Step 'the Make tab'                     { Ui-ClickLogged 'Ribbon tab MAKE' }
Ui-ExpectNew 'Ribbon tab shown: MAKE' 'the Make tab shown'
foreach ($group in 'MATERIALS', 'ANNOTATE', '3D PRINT', 'LASER') {
  Ui-ExpectNew "Ribbon MAKE/$group at" "its $group group"
}
if (Ui-LastLineWith 'Ribbon MAKE/CNC') { Ui-Fail 'a CNC group without commands' }
Ui-ExpectNew 'Browser Make markers: Root/Bodies/Body2: Own print settings: Walls' 'Body2 marked'
Group-Menu 'MAKE/MATERIALS' 'Edit Materials... | Material'
Group-Menu 'MAKE/ANNOTATE' 'Dimension | Note | Datum Label | Feature Control Frame | Hole Callout | Material | Process Note | - | Annotations...'
Group-Menu 'MAKE/3D PRINT' 'Print Analysis | Send to Slicer | Printers... | Processes... | Plates...'
Group-Menu 'MAKE/LASER' 'Export Flat Pattern... | Export Face as DXF...'

Write-Host '--- 2. The moved commands from their new places'
Ui-Mark
Ui-Step 'Edit Materials...'                { Ui-ClickLogged 'Ribbon button solid.materials' }
Ui-ExpectNew 'Materials opened' 'the Materials window'
Ui-Step 'close it (Escape)'                { Ui-FocusDialog '^Materials$'; Ui-Key 'Escape'; Ui-FocusMain }
Ui-Mark
Ui-Step 'Printers...'                      { Ui-ClickLogged 'Ribbon button make.printers' }
Ui-ExpectNew 'Printers opened' 'the Printers manager (mitcad#1135)'
Ui-Step 'close it (Escape)'                { Ui-FocusDialog '^Printers$'; Ui-Key 'Escape'; Ui-FocusMain }
Ui-Mark
Ui-Step 'Processes...'                     { Ui-ClickLogged 'Ribbon button make.processes' }
Ui-ExpectNew 'Processes opened' 'the Processes manager'
Ui-Step 'close it (Escape)'                { Ui-FocusDialog '^Processes$'; Ui-Key 'Escape'; Ui-FocusMain }
Ui-Mark
Ui-Step 'Print Analysis'                   { Ui-ClickLogged 'Ribbon button inspect.print_analysis' }
Ui-ExpectNew 'Command Print Analysis started' 'Print Analysis'
Ui-Step 'cancel (Escape)'                  { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Print Analysis cancelled' 'cancelled'
Ui-Mark
Ui-Step 'Send to Slicer'                   { Ui-ClickLogged 'Ribbon button make.print3d' }
Ui-ExpectNew 'Command Send to Slicer started' 'Send to Slicer (once 3D Print)'
Ui-Step 'cancel (Escape)'                  { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Send to Slicer cancelled' 'cancelled'
Ui-Mark
Ui-Step 'Export Flat Pattern...'           { Ui-ClickLogged 'Ribbon button sheet.export_dxf' }
Save-Path 'Export Flat Pattern' (Join-Path $work 'flat.dxf')
Ui-ExpectNew "Exported $workLogged/flat.dxf: dxf flat pattern of Body1" 'the flat pattern written' 30
# The faces' pick places are logged while a command's input takes faces:
# Dimension started and cancelled.
Ui-Mark
Ui-Step 'Dimension'                        { Ui-ClickLogged 'Ribbon button make.dimension' }
Ui-ExpectNew "Pick $top" 'the top face''s pick place'
Ui-Step 'cancel (Escape)'                  { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Dimension cancelled' 'cancelled'
Ui-Mark
Ui-Step 'select Body2''s top face'         { Ui-ClickPick $top }
Ui-ExpectNew 'Selected: 1 face' 'the top face selected'
Ui-Step 'Export Face as DXF...'            { Ui-ClickLogged 'Ribbon button make.export_face_dxf' }
Save-Path 'Export Face as DXF' (Join-Path $work 'face.dxf')
Ui-ExpectNew "Exported $workLogged/face.dxf: dxf outline of Body2 along F5:end(" 'the face''s outline written' 30
if (-not (Test-Path (Join-Path $work 'face.dxf'))) { Ui-Fail 'no face DXF' }
Ui-Step 'clear the selection (Escape)'     { Ui-Key 'Escape' }

Write-Host '--- 3. The Annotate group places each kind'
Annotate 'Ribbon button make.dimension' 'Dimension' $cbore
Ui-ExpectNew 'Dimension Selection 1: 1 face' 'the round face picked'
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: Diameter1' 'the counterbore''s diameter kept'
Annotate 'Ribbon button make.note' 'Note' $top
Ui-ExpectNew 'Note Pick: 1 face' 'the top face picked'
Ui-Step 'its text'                         { Ui-TypeIn 'Panel Note input text' 'Deburr' }
# Its preview before Add (mitcad#1097): the note the model places in the
# view, drawn dimmed; added, the note is where its preview was.
Ui-ExpectNew 'Annotation preview: Note1 note' 'the note previewed before Add'
Ui-ExpectNew 'Annotation preview drawn: Note1 = Deburr' 'the preview drawn with its text'
$preview = Ui-LoggedAt 'Annotation preview label Note1'
Ui-Mark
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: Note1' 'the note added'
Ui-ExpectNew 'Annotation preview drawn: none' 'the preview gone with the panel'
$added = Ui-LoggedAt 'Annotation label Note1'
if ($added[0] -ne $preview[0] -or $added[1] -ne $preview[1]) {
  Ui-Fail "Note1's text at $($added -join ','), its preview's at $($preview -join ',')"
}
Write-Host "ok   the note added where its preview was ($($added -join ','))"
Annotate 'shift+F3' 'Datum Label' $top
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: DatumA' 'the datum label added'
Annotate 'shift+F4' 'Feature Control Frame' $top
Ui-ExpectNew 'Panel Feature Control Frame action tolerance.0 at' 'Tolerance... placed'
Ui-Step 'Tolerance...'                     { Ui-ClickLogged 'Panel Feature Control Frame action tolerance.0' }
Ui-ExpectNew 'Feature Control Frame dialog opened: ' 'the frame''s dialog'
Ui-Step 'OK (Enter)'                       { Ui-FocusDialog '^Feature Control Frame$'; Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Feature Control Frame tolerance chosen: ' 'its tolerance chosen'
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: Frame1' 'the frame added'
Annotate 'shift+F5' 'Hole Callout' $cbore
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: HoleCallout1' 'the hole callout added'
Annotate 'shift+F6' 'Material' $top
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: Material1' 'the material note added'
Annotate 'shift+F7' 'Process Note' $top
Ui-Step 'add (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Annotation kept: ProcessNote1' 'the process note added'
Ui-ExpectNew 'ProcessNote1 = Printer: Bench / Process: Standard / Walls: 5' 'written from the body''s print settings'

Write-Host '--- 4. The selected body''s card'
Ui-Mark
Ui-Step 'select Body2 in the Browser'      { Ui-ClickLogged 'Browser Root/Bodies/Body2' }
Ui-ExpectNew 'printer Bench (component Root); process Standard (component Root); own: Walls = 5 (body Body2)' 'the card with Walls set on the body'
Ui-ExpectNew 'Make card value perimeters at' 'Walls'' field placed'
Ui-Mark
Ui-Step 'Walls 3'                          { Ui-ClickLogged 'Make card value perimeters'; Ui-SyncOr 200; Ui-Key 'ctrl+a'; Ui-Type '3'; Ui-Key 'Return' }
Ui-ExpectNew 'Make card Walls set: 3' 'set on the body'
Ui-ExpectNew 'own: Walls = 3 (body Body2)' 'the card shows 3'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'own: Walls = 5 (body Body2)' 'one undo step: 5 again'
Ui-ExpectNew 'Selection card section Make: ' 'the Make section''s place logged'
Section-Fits 'Make'

Write-Host '--- 4b. Add Print Value... and the remove button (mitcad#1178)'
Ui-Mark
Ui-Step 'select Body1 in the Browser'      { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Make card: Body1: ' 'Body1''s card'
Ui-ExpectNew 'process Standard (component Root); own: none' 'no print values of its own'
Ui-ExpectNew 'Make card add value at' 'Add Print Value... placed'
# The row is logged only when it changes, from "none" at start: no line
# yet means none.
$create = "$(Ui-LastLineWith 'Selection card create: ' -Start)"
if ($create -ne '' -and $create -ne 'Selection card create: none') { Ui-Fail "a Create row on the Make tab: $create" }
Write-Host 'ok   no Create row on the Make tab'
Ui-Mark
Ui-Step 'Add Print Value...'               { Ui-ClickLogged 'Make card add value' }
Ui-ExpectNew 'Add print value opened for Body1: ' 'the picker opened'
Ui-Step 'search Walls'                     { Ui-FocusDialog '^Add Print Value$'; Ui-Type 'Walls' }
Ui-ExpectNew 'Add print value search: Walls: ' 'Walls searched'
$search = "$(Ui-LastLineWith 'Add print value search: ')"
if (-not $search.Contains(', current perimeters')) { Ui-Fail "Walls is not the current setting: $search" }
Write-Host 'ok   Walls first'
Ui-Step 'add (Enter)'                      { Ui-Key 'Return'; Ui-FocusMain }
Ui-ExpectNew 'Add print value chose perimeters' 'Walls chosen'
Ui-ExpectNew 'Make card Walls added: 2' 'added to Body1 at the process''s 2'
Ui-ExpectNew 'process Standard (component Root); own: Walls = 2 (body Body1)' 'its row on the body'
Ui-ExpectNew 'Make card value perimeters icon body' 'with the body''s icon'
Ui-ExpectNew 'Make card value perimeters focused' 'its field focused'
Ui-ExpectNew 'Make card remove perimeters at' 'its remove button'
Ui-ExpectNew 'Root/Bodies/Body1: Own print settings: Walls' 'the Browser marks Body1'
Ui-Mark
Ui-Step 'Walls 4'                          { Ui-ClickLogged 'Make card value perimeters'; Ui-SyncOr 200; Ui-Key 'ctrl+a'; Ui-Type '4'; Ui-Key 'Return' }
Ui-ExpectNew 'Make card Walls set: 4' '4 typed is set on the body'
Ui-ExpectNew 'own: Walls = 4 (body Body1)' 'the card shows 4'
Ui-ExpectNew 'Selection card section Make: ' 'the Make section''s place logged'
Section-Fits 'Make'
Ui-Mark
Ui-Step 'remove Walls'                     { Ui-ClickLogged 'Make card remove perimeters' }
Ui-ExpectNew 'Make card Walls removed' 'the remove button clears it on the body'
Ui-ExpectNew 'process Standard (component Root); own: none' 'no row: the process''s Walls again'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'own: Walls = 4 (body Body1)' 'one undo step: 4 again'
Ui-Mark
Ui-Step 'the Solid tab'                    { Ui-ClickLogged 'Ribbon tab SOLID' }
Ui-ExpectNew 'Make card: none' 'no card on the Solid tab'
Ui-ExpectNew 'Browser Make markers: none' 'no markers on the Solid tab'

Write-Host '--- 4c. The card''s Printer and Process pickers (mitcad#1239)'
# Picker-Choose "Printer" "typed": the card's picker opened, the text typed
# in its search, Enter.
function Picker-Choose([string]$Kind, [string]$Typed, [string]$First) {
  Ui-Mark
  Ui-Step "open the $Kind picker"          { Ui-ClickLogged "Make card $($Kind.ToLower())" }
  Ui-ExpectNew "$Kind picker opened: " "the card's $Kind picker opened"
  Ui-FocusDialog "^$Kind Picker$"
  Ui-Mark
  Ui-Step "type $Typed"                    { Ui-Type $Typed }
  Ui-ExpectNew "$Kind picker search: $Typed, first $First" "$First found"
  Ui-Mark
  Ui-Step 'choose (Enter)'                 { Ui-Key 'Return' }
  Ui-ExpectNew "$Kind picker chose: " 'chosen'
  Ui-FocusMain
}
function Printer-Choose {
  Picker-Choose 'Printer' 'Bench' 'Bench'
  Ui-ExpectNew 'Make card Printer set: Bench' 'set as the design''s printer'
  Ui-ExpectNew 'printer Bench (component Root); process Slicer default (default)' 'the card shows it at once, chosen on the root'
}
Ui-StartApp 'make-tab-pickers' @('--open', $pickers, '--no-native-dialogs')
Ui-ExpectLog "Opened $workLogged/pickers.mitcad" 'the design without a printer opened'
Ui-Mark
Ui-Step 'the Make tab'                     { Ui-ClickLogged 'Ribbon tab MAKE' }
Ui-ExpectNew 'Ribbon tab shown: MAKE' 'the Make tab shown'
Ui-Mark
Ui-Step 'select Body1 in the Browser'      { Ui-ClickLogged 'Browser Root/Bodies/Body1' }
Ui-ExpectNew 'Make card: Body1: material ' 'Body1''s card'
Ui-ExpectNew 'printer Slicer default (default); process Slicer default (default)' 'no printer and no process chosen'
Ui-ExpectNew 'Make card rows: Printer, Process, Material' 'the rows from the widest choice: Printer, Process, Material'
$printers = "$(Ui-LastLineWith 'Make card printers: ')"
if (-not $printers.Contains('Bench')) { Ui-Fail "the Printer picker lists '$printers'" }
Write-Host "ok   the Printer picker lists the printers: $printers"
Ui-ExpectNew 'Make card processes: Alpha: Coarse @Alpha | Bench: Fine @Bench' 'the library''s processes under their printers'
Printer-Choose
Ui-ExpectNew 'Make card processes: Bench: Fine @Bench | Alpha: Coarse @Alpha' 'the design''s printer''s processes first now (mitcad#1216)'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'printer Slicer default (default); process Slicer default (default)' 'one undo step: Slicer default again'
Printer-Choose
Ui-Mark
Ui-Step 'open the Process picker'          { Ui-ClickLogged 'Make card process' }
Ui-ExpectNew 'Process picker sections: Bench 1 | Alpha 1' 'Bench''s library process first'
Ui-FocusDialog '^Process Picker$'
Ui-Mark
Ui-Step 'type @Bench'                      { Ui-Type '@Bench' }
Ui-ExpectNew 'Process picker search: @Bench, first ' 'the process for Bench found'
Ui-Mark
Ui-Step 'choose (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew 'Process picker chose: ' 'chosen'
Ui-FocusMain
Ui-ExpectNew 'Make card Process set: Fine @Bench' 'the library''s process chosen'
Ui-ExpectNew 'printer Bench (component Root); process Fine @Bench (body Body1)' 'copied into the design and set on the body'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                    { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'printer Bench (component Root); process Slicer default (default)' 'one undo step: the body without a process again'
Picker-Choose 'Printer' 'Printers' "Printers$ellipsis"
Ui-ExpectNew 'Make card printer manager: design/bench' 'Printers... on the design''s printer'
Ui-ExpectNew 'Printers opened' 'the Printers manager opened'
Ui-ExpectNew 'Profile selected: printer Bench (design)' 'on Bench'
Ui-Step 'close it (Escape)'                { Ui-FocusDialog '^Printers$'; Ui-Key 'Escape'; Ui-FocusMain }
if (@(Ui-NewLines | Where-Object { $_.Contains('Make card Printer set: ') }).Count -gt 0) { Ui-Fail 'Printers... changed the printer' }
Write-Host 'ok   Printers... keeps the printer'

Write-Host '--- 5. Floating'
Ui-StartApp 'make-tab-floating' @('--open', $design, '--chrome=floating')
Ui-ExpectLog 'Ribbon tab MAKE at' 'the Make tab in the Floating layout'
Ui-Mark
Ui-Step 'the Make tab'                     { Ui-ClickLogged 'Ribbon tab MAKE' }
foreach ($group in 'MATERIALS', 'ANNOTATE', '3D PRINT', 'LASER') {
  Ui-ExpectNew "Ribbon MAKE/$group at" "its $group group, Floating"
}
Ui-Mark
Ui-Step 'Dimension'                        { Ui-ClickLogged 'Ribbon button make.dimension' }
Ui-ExpectNew 'Command Dimension started' 'Dimension from the Floating toolbar'
Ui-Step 'cancel (Escape)'                  { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Dimension cancelled' 'cancelled'

Ui-Finish 'the Make tab'
