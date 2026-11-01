# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/MainWindowMake.cpp app/print/MakeBodyCard.cpp app/browser/BrowserPanel.cpp
# check-all sources: core/model-api/src/body_notes.rs core/model-api/src/cut_sheets.rs tools/cli
# check-all sources: core/model-api/src/annotations.rs app/MainWindowMeasurements.cpp app/view/MeasurementOverlay.cpp
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
#      Edit Printers and Processes..., Print Analysis, Send to Slicer and
#      Export Flat Pattern... (its DXF written); Export Face as DXF... on
#      Body2's top face.
#   3. The Annotate group places each kind: Dimension and Note from their
#      buttons (the note previewed before Add, then added where its preview
#      was, mitcad#1097), Datum Label, Feature Control Frame, Hole Callout,
#      Material and Process Note by shortcuts of the test's (their menu entries are
#      in the Linux test: the input goes to the main window's places only).
#   4. Body2's card on the Make tab: Walls 5 set on the body, typed 3 there,
#      one Undo back to 5; none on the Solid tab, nor markers.
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
  {"cmd":"set_property","owner":"F5.b0","set":"Print Process","key":"perimeters","value":5}
]
'@
$output = & $cli run $modelJson --save $design 2>&1
if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli: $($output -join ' / ')" }

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
Group-Menu 'MAKE/3D PRINT' 'Print Analysis | Send to Slicer | Edit Printers and Processes...'
Group-Menu 'MAKE/LASER' 'Export Flat Pattern... | Export Face as DXF...'

Write-Host '--- 2. The moved commands from their new places'
Ui-Mark
Ui-Step 'Edit Materials...'                { Ui-ClickLogged 'Ribbon button solid.materials' }
Ui-ExpectNew 'Materials opened' 'the Materials window'
Ui-Step 'close it (Escape)'                { Ui-FocusDialog '^Materials$'; Ui-Key 'Escape'; Ui-FocusMain }
Ui-Mark
Ui-Step 'Edit Printers and Processes...'   { Ui-ClickLogged 'Ribbon button make.printers' }
Ui-ExpectNew 'Printers and processes opened' 'Printers and Processes'
Ui-Step 'close it (Escape)'                { Ui-FocusDialog '^Printers and Processes$'; Ui-Key 'Escape'; Ui-FocusMain }
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
Ui-Mark
Ui-Step 'the Solid tab'                    { Ui-ClickLogged 'Ribbon tab SOLID' }
Ui-ExpectNew 'Make card: none' 'no card on the Solid tab'
Ui-ExpectNew 'Browser Make markers: none' 'no markers on the Solid tab'

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
