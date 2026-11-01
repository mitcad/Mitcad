# SPDX-License-Identifier: MIT
# check-all sources: app/browser/PropertySetsDialog.cpp app/browser/BrowserController.cpp app/browser/FindDialog.cpp
# check-all sources: app/browser/FindInDesign.cpp app/build/BuildLists.cpp app/MainWindow.cpp tools/cli
# check-all sources: core/model-base/src/properties.rs core/model/src/document/properties.rs core/model-api/src/properties.rs
# check-all sources: app/browser/DesignPropertiesDialog.cpp app/files/MainWindowFiles.cpp core/model/src/document/design_summary.rs core/model-api/src/design_properties.rs
# Input-driven Windows UI test of Property Sets (mitcad#496) in the build
# VM (tools/ui-windows-lib.ps1 says how the input is posted), on a design
# mitcad-cli makes, as tools/ui-property-sets-test.sh does on Linux: a
# component Wall placed twice (Wall:1, Wall:2) with the body Panel, an own
# type FireRating (EI30 / EI60 / EI90), FireRating EI30 and Mark W-12 on
# the component, Mark W-13 on Wall:1.
#   0. File > Design Properties... (a shortcut of the test's, mitcad#1088)
#      sets Title and Author; Property Sets with nothing selected (the
#      root's) shows them in its set Summary.
#   1. Wall:1 selected in the browser, Property Sets (a shortcut of the
#      test's): the occurrence's and the component's levels, Wall:1's own
#      Mark over the component's, FireRating inherited.
#   2. Mark edited in place (its row found by typing its name, F2): W-30.
#   3. The next FireRating typed (the component's, after Wall:1's Mark), F2:
#      its type's values as a choice; the next one (Down: the list cannot
#      open in session 0) sets EI60, which Wall:1 inherits.
#   4. Find in Design (a shortcut) by property: FireRating = EI60 finds
#      both occurrences.
#   5. The Bill of Materials (a shortcut) takes Mark and FireRating as columns:
#      the component's W-12 and EI60.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-property-sets-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-property-sets'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'view.find_in_design' = 'F7'; 'view.property_sets' = 'F8'; 'build.parts_list' = 'F9'
  'file.design_properties' = 'F11' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'properties.mitcad'
Set-Content -Encoding ascii $modelScript (@'
[
  {"cmd": "create_component", "name": "Wall"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Panel"},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "Wall:1", "transform": {"translation": [60, 0, 0]}},
  {"cmd": "define_property_type", "name": "FireRating", "base": "text",
   "values": ["EI30", "EI60", "EI90"]},
  {"cmd": "set_property", "owner": "C1", "set": "Pset_WallCommon", "key": "FireRating",
   "type": "FireRating", "value": "EI30"},
  {"cmd": "set_property", "owner": "C1", "set": "Identity", "key": "Mark", "value": "W-12"},
  {"cmd": "set_property", "owner": "O1", "set": "Identity", "key": "Mark", "value": "W-13"}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'property-sets' @('--open', $design)
Ui-ExpectLog 'Browser Root/Wall:1 at' 'Wall:1 is in the browser'

Write-Host '--- 0. Design Properties: the summary on the root (mitcad#1088)'
Ui-Mark
Ui-Step 'Design Properties (F11)'        { Ui-Key 'F11' }
Ui-ExpectNew 'Design Properties opened: Created unknown, Last saved unknown' 'nothing known of the statistics yet'
Ui-FocusDialog '^Design Properties$'
Ui-Step 'Title (the field has the focus)' { Ui-Type 'Wall study' }
Ui-ExpectNew 'Dialog field Design Properties: Wall study' 'the Title typed'
Ui-Step 'to Author (Tab, Tab)'           { Ui-Key 'Tab'; Ui-Key 'Tab' }
Ui-Step 'Author'                         { Ui-Type 'A. Maker' }
Ui-ExpectNew 'Dialog field Design Properties: A. Maker' 'the Author typed'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Design Properties set: Title, Author' 'one command with the two fields'
Ui-FocusMain
Ui-Mark
Ui-Step 'Property Sets, nothing selected (F8)' { Ui-Key 'F8' }
Ui-ExpectNew 'Property Sets opened for C0' 'the design''s (the root''s)'
Ui-ExpectNew 'Property sets row: component Summary/Title = Wall study (user)' 'the Title on the root'
Ui-ExpectNew 'Property sets row: component Summary/Author = A. Maker (user)' 'the Author on the root'
Ui-FocusDialog '^Property Sets$'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }
Ui-FocusMain

Write-Host '--- 1. The occurrence''s and the component''s levels'
Ui-Mark
Ui-Step 'select Wall:1'                  { Ui-ClickLogged 'Browser Root/Wall:1' }
Ui-Step 'Property Sets (F8)'             { Ui-Key 'F8' }
Ui-ExpectNew 'Property Sets opened for O1' 'the window opened for Wall:1'
Ui-ExpectNew 'Property sets row: occurrence Identity/Mark = W-13 (user, overrides)' 'Wall:1''s own Mark'
Ui-ExpectNew 'Property sets row: occurrence Pset_WallCommon/FireRating = EI30 (user, inherited)' 'FireRating inherited'
Ui-ExpectNew 'Property sets row: component Identity/Mark = W-12 (user)' 'the component''s Mark'

Write-Host '--- 2. A value edited in place'
Ui-FocusDialog '^Property Sets$'
Ui-Mark
Ui-Step 'go to Mark (type its name)'     { Ui-Type 'Mark' }
Start-Sleep -Milliseconds 800
Ui-Step 'edit (F2)'                      { Ui-Key 'F2' }
Ui-ExpectNew 'Editing property occurrence Identity/Mark' 'Wall:1''s Mark in edit'
Ui-Step 'W-30 (Enter)'                   { Ui-Key 'ctrl+a'; Ui-Type 'W-30'; Ui-Key 'Return' }
Ui-ExpectNew 'Property occurrence Identity/Mark set: W-30' 'the edit is a model command'
Ui-ExpectNew 'Property sets row: occurrence Identity/Mark = W-30 (user, overrides)' 'Wall:1''s Mark is W-30'

Write-Host '--- 3. An own type''s value as a choice'
Ui-FocusDialog '^Property Sets$'
Ui-Mark
Start-Sleep -Milliseconds 800
Ui-Step 'go to the next FireRating'      { Ui-Type 'FireRating' }
Start-Sleep -Milliseconds 800
Ui-Step 'edit (F2)'                      { Ui-Key 'F2' }
Ui-ExpectNew 'Editing property component Pset_WallCommon/FireRating as a choice: EI30 | EI60 | EI90' 'its type''s values offered'
Ui-Step 'the next value (Down)'           { Ui-Key 'Down' }
Ui-ExpectNew 'Property component Pset_WallCommon/FireRating chosen: EI60' 'EI60 chosen'
Ui-ExpectNew 'Property sets row: component Pset_WallCommon/FireRating = EI60 (user)' 'the component''s FireRating is EI60'
Ui-ExpectNew 'Property sets row: occurrence Pset_WallCommon/FireRating = EI60 (user, inherited)' 'Wall:1 inherits it'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Write-Host '--- 4. Find in Design by property'
Ui-FocusMain
Ui-Mark
Ui-Step 'Find in Design (F7)'            { Ui-Key 'F7' }
Ui-ExpectNew 'Find in Design opened' 'the window opened'
Ui-FocusDialog '^Find in Design$'
Ui-Mark
Ui-Step 'FireRating = EI60'              { Ui-Type 'FireRating = EI60' }
Ui-ExpectNew 'Find in Design found: 0 bodies, 2 components' 'both occurrences inherit EI60'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Write-Host '--- 5. The Bill of Materials'' property columns'
Ui-FocusMain
Ui-Mark
Ui-Step 'Bill of Materials (F9)'                { Ui-Key 'F9' }
Ui-FocusDialog '^Bill of Materials$'
Ui-ExpectNew 'Build parts list: ' 'the list opened'
Ui-Mark
Ui-Step 'the columns field (Alt+L)'      { Ui-Key 'alt+l' }
Ui-Step 'Mark, FireRating (Enter)'       { Ui-Type 'Mark, FireRating'; Ui-Key 'Return' }
Ui-ExpectNew 'Build parts list columns: Mark, FireRating' 'two property columns'
Ui-ExpectNew 'Build parts list row: Wall: W-12 | EI60' 'the component''s values in them'
Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }

Ui-Finish 'UI Windows property sets test'
