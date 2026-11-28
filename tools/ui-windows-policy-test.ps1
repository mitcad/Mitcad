# SPDX-License-Identifier: MIT
# check-all sources: app/browser/PolicyDialog.cpp app/browser/BrowserController.cpp app/MainWindow.cpp
# check-all sources: core/model/src/policy.rs core/model/src/document/policy.rs core/model-api/src/policy.rs
# check-all sources: core/model/src/document/components.rs core/model-api/src/components.rs
# check-all sources: app/commands/PrimitiveCommands.cpp
# Input-driven Windows UI test of the design policy (mitcad#220, mitcad#421)
# in the build VM (tools/ui-windows-lib.ps1 says how the input is posted):
#   1. A new design has Component1 under the root, active: the first sketch and
#      the extrusion of a 40 x 25 rectangle go into Component1.
#   2. The root, Hybrid like Component1 (everything allowed, mitcad#421), takes
#      sketches. Design Policy (a shortcut of the test's) of the root:
#      Hybrid with no violations; Assembly (Alt+A), Hybrid (Alt+H) and
#      Assembly again; OK sets it, and Create Sketch is refused with the
#      reason, nothing redirected; Undo takes the policy back and the root
#      takes sketches again.
#   3. New Component Part2 in the root, Hybrid: its first sketch and its
#      next Create Sketch can lie on Component1's faces. Set to Part (Alt+P), its
#      attached first sketch breaks nothing; the Policy dialog counts it
#      against the first-sketch setting when that is off (Alt+I) and sets
#      it; its next Create Sketch then leaves Component1's faces out of the picks
#      and names Component1 in the hint. Component1 set to Part with first sketches
#      from others off (Alt+P, Alt+K) counts Part2's first sketch; with
#      attachments from others on (Alt+T) and Part2 attaching to others
#      (Alt+O), Component1's faces can be picked again, and the sketch goes into
#      Part2 on it.
#   4. Component1 renamed Bracket in the browser (F2): its only body, Body1, is
#      renamed with it (mitcad#291).
#   5. Bracket (a part) active, New Component Inner: made beside it in the
#      root, with the warning (mitcad#292).
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-policy-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-policy'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'sketch.create' = 'F7'; 'assembly.policy' = 'F8'; 'solid.new_component' = 'F12' } -StepPause 0

# Ui-ExpectNone "text" "description": no line since the mark has the text.
function Ui-ExpectNone([string]$Text, [string]$Description) {
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains($Text) }) {
    Ui-Fail "${Description}: '$Text' is in the log"
  }
  Write-Host "ok   $Description"
}

# Design Policy of the active component: opens the dialog, runs the keys
# there and closes it with OK.
function Set-Policy([string]$Name, [string[]]$Keys, [string]$Expected) {
  Ui-Mark
  Ui-Step "design policy of $Name (F8)" { Ui-Key 'F8' }
  Ui-ExpectNew "Design Policy dialog opened for $Name" "the policy of $Name opened"
  Ui-FocusDialog "^Design Policy of $Name$"
  # A key's button or check box clicks a moment after the key (Qt's
  # animateClick), and Enter clicks OK at once: each key waits for the
  # dialog's line of its change.
  foreach ($key in $Keys) {
    Ui-Mark
    Ui-Step "press $key" { Ui-Key $key }
    Ui-ExpectNew "Policy dialog ${Name}: " "the dialog took $key"
  }
  Ui-Step 'OK (Enter)'                   { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Policy of $Name set: $Expected" "the policy of $Name set"
}

Write-Host '--- A new design: Component1 in the root, active'
Ui-StartApp 'policy'
Ui-ExpectLog 'Browser Root/Component1:1 at' 'Component1 is in the browser'
Ui-ExpectLog 'Browser radio Root at' 'the root is not the active component'
Ui-CreateSketch 'F7' 'xy'
Ui-ExpectLog 'Sketch Sketch1 is in Component1' 'the first sketch went into Component1'
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner at the origin'     { Ui-SketchClick 0 0 }
Ui-Step 'width 40 (Tab) height 25'       { Ui-Type '40'; Ui-Key 'Tab'; Ui-Type '25' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectLog 'Added rectangle 40 x 25 mm at (0, 0)' 'rectangle 40 x 25 at the origin'
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectLog 'Sketch finished' 'sketch finished'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with the new profile'
Ui-Step 'type the distance 20'           { Ui-Type '20' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (New Body), distance 20' 'the extrusion built in Component1'

Write-Host '--- The root: Hybrid by default, Assembly refuses sketches'
Ui-Mark
Ui-Step 'activate the root'              { Ui-ClickLogged 'Browser radio Root' }
Ui-ExpectNew 'Activated Root' 'the root is active'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: select a plane' 'the root takes sketches (Hybrid)'
Ui-ExpectNone 'Create Sketch refused' 'nothing refused'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-Mark
Ui-Step 'design policy (F8)'             { Ui-Key 'F8' }
Ui-ExpectNew 'Policy dialog Root: hybrid; sketches allowed, features allowed, components allowed' 'the root is Hybrid'
Ui-FocusDialog '^Design Policy of Root$'
Ui-Mark
Ui-Step 'Assembly preset (Alt+A)'        { Ui-Key 'alt+a' }
Ui-ExpectNew 'Policy dialog Root: assembly; sketches blocked (0), features blocked (0), components allowed' 'Assembly, with no violations'
Ui-Mark
Ui-Step 'Hybrid preset (Alt+H)'          { Ui-Key 'alt+h' }
Ui-ExpectNew 'Policy dialog Root: hybrid; sketches allowed' 'Hybrid again'
Ui-Mark
Ui-Step 'Assembly preset (Alt+A)'        { Ui-Key 'alt+a' }
Ui-ExpectNew 'Policy dialog Root: assembly;' 'Assembly again'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Policy of Root set: assembly' 'the root is an assembly now'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch refused: Root does not allow sketches' 'refused with the reason'
Ui-ExpectNone 'Create Sketch: select a plane' 'no plane is asked for'
Ui-ExpectNone 'Activated ' 'nothing else is activated'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Policy of Root' 'the policy change undone'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: select a plane' 'the root takes sketches again'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Write-Host '--- Sketch planes of other components'
Ui-Mark
Ui-Step 'new component (F12)'             { Ui-Key 'F12' }
Ui-ExpectNew 'Panel New Component input name at' 'the New Component panel'
Ui-Step 'name it Part2'                  { Ui-TypeIn 'Panel New Component input name' 'Part2' }
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Browser Root/Part2:1 at' 'Part2 in the browser'
Ui-ExpectNew 'Browser radio Root at' 'Part2 is active, not the root'
# Part2's first sketch may lie on Component1's face.
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F2.b0/F2:side(c1[c4,c2]) in O1 at' "Component1's faces can be picked for Part2's first sketch"
Ui-ExpectNone 'Create Sketch: policy leaves out' 'nothing left out'
Ui-Step "pick a side face of Component1's body" { Ui-ClickPick 'face F2.b0/F2:side(c1[c4,c2])' }
Ui-ExpectNew 'Sketch Sketch2 is in Part2' 'the first sketch went into Part2'
Ui-ExpectNew ' in O1, origin ' 'on the face where Component1:1 shows it'
# Something in it: Finish Sketch takes an empty sketch back, and Part2
# would have no first sketch.
Ui-Mark
Ui-Step 'rectangle tool (R)'             { Ui-Key 'r' }
Ui-Step 'first corner at the origin'     { Ui-SketchClick 0 0 }
Ui-Step 'width 5 (Tab) height 5'         { Ui-Type '5'; Ui-Key 'Tab'; Ui-Type '5' }
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Added rectangle 5 x 5 mm at (0, 0)' 'a 5 x 5 rectangle in Sketch2'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'     { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
# Its next sketch too, under the defaults (mitcad#421).
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F2.b0/F2:side(c1[c4,c2]) in O1 at' "Component1's faces can be picked for Part2's next sketch"
Ui-ExpectNone 'Create Sketch: policy leaves out' 'nothing left out'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Write-Host '--- The Component preset and the first-sketch settings in the Policy dialog (mitcad#293)'
Ui-Mark
Ui-Step 'design policy of Part2 (F8)'    { Ui-Key 'F8' }
Ui-ExpectNew 'Design Policy dialog opened for Part2' 'the policy of Part2 opened'
Ui-ExpectNew 'Policy dialog Part2: hybrid;' 'a new component is Hybrid'
Ui-FocusDialog '^Design Policy of Part2$'
Ui-Mark
Ui-Step 'Component preset (Alt+N)'            { Ui-Key 'alt+n' }
Ui-ExpectNew 'Policy dialog Part2: part;' 'Part2 is a part'
Ui-ExpectNew 'attach_to_others blocked (0), attachments_from_others blocked (0), first_sketch_attaches allowed, first_sketch_attachments_from_others allowed' 'the attached first sketch breaks nothing'
Ui-Mark
Ui-Step 'first sketch may attach off (Alt+I)' { Ui-Key 'alt+i' }
Ui-ExpectNew 'Policy dialog Part2: custom;' 'Part2 is custom'
Ui-ExpectNew 'first_sketch_attaches blocked (1)' 'Sketch2 counted against it'
Ui-Mark
Ui-Step 'first sketch may attach on (Alt+I)' { Ui-Key 'alt+i' }
Ui-ExpectNew 'Policy dialog Part2: part;' 'a part again'
Ui-ExpectNew 'first_sketch_attaches allowed' 'allowed again'
Ui-Mark
Ui-Step 'first sketch may attach off (Alt+I)' { Ui-Key 'alt+i' }
Ui-ExpectNew 'first_sketch_attaches blocked (1)' 'counted again'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Policy of Part2 set: custom' 'the policy of Part2 set'
Ui-Mark
Ui-Step 'design policy of Part2 (F8)'    { Ui-Key 'F8' }
Ui-ExpectNew 'Design Policy dialog opened for Part2' 'the policy of Part2 opened again'
Ui-ExpectNew 'first_sketch_attaches blocked (1)' 'the setting stays off, Sketch2 counted'
Ui-FocusDialog '^Design Policy of Part2$'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Design Policy dialog closed without a change' 'nothing changed'
# Its next sketch follows the sketch attachment pair: Component1 is left out.
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Create Sketch: policy leaves out Component1' "Component1's faces left out for the next sketch, named"
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Start-Sleep -Milliseconds 500
Ui-ExpectNone 'Pick face F2.b0/' "no face of Component1's body can be picked"
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }

Ui-Mark
Ui-Step 'activate Component1'            { Ui-ClickLogged 'Browser radio Root/Component1:1' }
Ui-ExpectNew 'Activated Component1' 'Component1 is active'
Ui-Mark
Ui-Step 'design policy of Component1 (F8)' { Ui-Key 'F8' }
Ui-ExpectNew 'Design Policy dialog opened for Component1' 'the policy of Component1 opened'
Ui-ExpectNew 'Policy dialog Component1: hybrid;' 'Component1 is Hybrid'
Ui-FocusDialog '^Design Policy of Component1$'
Ui-Mark
Ui-Step 'Component preset (Alt+N)'            { Ui-Key 'alt+n' }
Ui-ExpectNew 'Policy dialog Component1: part;' 'Component1 is a part'
Ui-Mark
Ui-Step 'first sketches from others off (Alt+K)' { Ui-Key 'alt+k' }
Ui-ExpectNew 'first_sketch_attachments_from_others blocked (1)' "Part2's first sketch counted against it"
Ui-Mark
Ui-Step 'attachments from others on (Alt+T)' { Ui-Key 'alt+t' }
Ui-ExpectNew 'attachments_from_others allowed, first_sketch_attaches allowed, first_sketch_attachments_from_others blocked (0)' 'allowed by the general setting'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Policy of Component1 set: custom' 'the policy of Component1 set'
Ui-Mark
Ui-Step 'design policy of Component1 (F8)' { Ui-Key 'F8' }
Ui-ExpectNew 'Design Policy dialog opened for Component1' 'the policy of Component1 opened again'
Ui-ExpectNew 'first_sketch_attaches allowed, first_sketch_attachments_from_others blocked (0)' 'the first-sketch setting stays off'
Ui-FocusDialog '^Design Policy of Component1$'
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Design Policy dialog closed without a change' 'nothing changed'

Ui-Mark
Ui-Step 'activate Part2'                 { Ui-ClickLogged 'Browser radio Root/Part2:1' }
Ui-ExpectNew 'Activated Part2' 'Part2 is active'
Set-Policy 'Part2' @('alt+o') 'custom'
Ui-Mark
Ui-Step 'create sketch (F7)'             { Ui-Key 'F7' }
Ui-ExpectNew 'Pick places:' 'the picks are logged'
Ui-ExpectNew 'Pick face F2.b0/F2:side(c1[c4,c2]) in O1 at' "Component1's faces can be picked"
Ui-ExpectNone 'Create Sketch: policy leaves out' 'nothing left out'
Ui-Step "pick a side face of Component1's body" { Ui-ClickPick 'face F2.b0/F2:side(c1[c4,c2])' }
Ui-ExpectNew 'Sketch Sketch3 is in Part2' 'the sketch went into Part2'
Ui-ExpectNew ' in O1, origin ' 'on the face where Component1:1 shows it'

Write-Host '--- Renaming Component1 renames its only body (mitcad#291)'
Ui-Mark
Ui-Step 'finish sketch (Ctrl+Enter)'      { Ui-Key 'ctrl+Return' }
Ui-ExpectNew 'Sketch finished' 'sketch finished'
Ui-Mark
Ui-Step 'select Component1:1 in the browser' { Ui-ClickLogged 'Browser Root/Component1:1' }
Ui-Step 'rename (F2)'                     { Ui-Key 'F2' }
Ui-ExpectNew 'Renaming Root/Component1:1' 'Component1 renamed in place'
Ui-Step 'type Bracket, Enter'             { Ui-Type 'Bracket'; Ui-Key 'Return' }
Ui-ExpectNew 'Renamed Component1:1 to Bracket' 'Component1 is Bracket'
Ui-ExpectNew 'Renamed body F2.b0 with its component to Bracket' 'Body1 renamed with its component'
Ui-ExpectNew 'Browser Root/Bracket:1/Bodies/Bracket at' "the browser shows the body's new name"

Write-Host '--- New Component in Bracket, a part: made beside it (mitcad#292)'
Ui-Mark
Ui-Step 'activate Bracket'                { Ui-ClickLogged 'Browser radio Root/Bracket:1' }
Ui-ExpectNew 'Activated Bracket' 'Bracket is active'
Ui-Mark
Ui-Step 'new component (F12)'              { Ui-Key 'F12' }
Ui-ExpectNew 'Panel New Component input name at' 'the New Component panel'
Ui-Step 'name it Inner'                   { Ui-TypeIn 'Panel New Component input name' 'Inner' }
Ui-Step 'OK (Enter)'                      { Ui-Key 'Return' }
Ui-ExpectNew 'Placement warning: Bracket does not take components (its policy): New Component made in Root instead' 'made in the root, with the warning'
# The new row is the last: scrolled into sight when the tree is taller
# than the panel.
for ($i = 0; $i -lt 6; $i++) {
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains('Browser radio Root/Inner:1 at') }) { break }
  Ui-WheelLogged 'Browser Root' -4
  Start-Sleep -Milliseconds 500
}
Ui-ExpectNew 'Browser radio Root/Inner:1 at' 'Inner is in the root'
Ui-ExpectNone 'Browser radio Root/Bracket:1/Inner:1' 'not under Bracket'

Ui-Finish 'UI Windows policy test'
