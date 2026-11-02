# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowContextMenu.cpp app/MainWindow.cpp app/files/MainWindowPrint.cpp
# check-all sources: app/commands/InspectCommands.cpp app/browser core/model/src/document/print_bed.rs
# check-all sources: core/model/src/exchange core/3mf tools/cli
# check-all sources: app/files/Print3d.cpp app/framework/CommandSession.cpp app/framework/DocumentSession.cpp
# Input-driven Windows UI test of a body's print bed face (mitcad#731) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-print-bed-test.sh does on Linux, on a 40 x 20 x 10 box (Plate,
# F1.b0) that mitcad-cli makes, with a fake slicer (a PowerShell script
# that writes the arguments it was started with). The context menu stays
# open in session 0 and takes the keys (as in
# tools/ui-windows-sketch-inactive-test.ps1).
#   1. The context menu on the box's top offers Set as Print Bed Face (and
#      no Clear); it sets the face, and the saved file keeps it.
#   2. The context menu on a side shows the top as the print bed face while
#      it is open.
#   3. The context menu on the top offers Clear Print Bed Face; it clears
#      the face, and Undo brings it back.
#   4. 3D Print (a shortcut of the test's) to a 3MF file turns the box over
#      onto its top and centres it on the 256 mm plate.
#   5. Print Analysis (a shortcut) starts with the top as its bed face.
#   6. A design whose stored face is gone: 3D Print reports it and lays
#      the box on its bottom by the automatic orientation (mitcad#1109).
#      A side's menu offers Clear Print Bed Face (beside Set); cleared, 3D
#      Print warns no more.
#   7. A box made a surface body (its bottom unstitched) that keeps its
#      print bed face (mitcad#737): the menus of its top and of a side
#      offer Clear Print Bed Face only, show no bed face, and Clear clears
#      it; then the top's menu offers neither, and the status line says
#      why the surface body has no print bed face (mitcad#1182).
#   8. Send to Slicer's Bed Face (mitcad#1109) on a box without a print
#      bed face: choosing the body fills it with the automatic
#      orientation's face (its bottom), highlighted while the panel is
#      open; picking the top in Bed Face stores it as the body's print bed
#      face (one undo step), the highlight follows, and Send turns the box
#      onto it.
#   9. The bed plan in the background (mitcad#1121), made slow
#      (MITCAD_TEST_JOB_SLEEP_MS="bed plan=2500"): Bed Face says Planning,
#      Enter posted while the plan runs sends as soon as it is done (one
#      log line says it waited), and no progress dialog shows meanwhile.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-print-bed-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-print-bed'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.offset_face' = 'F7'; 'make.print3d' = 'F8'; 'inspect.print_analysis' = 'F9' } -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

# The app's temporary folder of the test's own, and a fake slicer in the
# settings.
$printTemp = Join-Path $script:UiOut 'tmp'
Remove-Item -Recurse -Force $printTemp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $printTemp | Out-Null
$env:TMP = $printTemp
$env:TEMP = $printTemp
$slicerArgs = Join-Path $script:UiOut 'slicer-args.txt'
Remove-Item -Force $slicerArgs, "$slicerArgs.part" -ErrorAction SilentlyContinue
$slicerScript = Join-Path $script:UiOut 'fake-slicer.ps1'
Set-Content -Encoding ascii $slicerScript (@'
[IO.File]::WriteAllLines('ARGS.part', [string[]]$args)
Move-Item -Force 'ARGS.part' 'ARGS'
'@ -creplace 'ARGS', $slicerArgs)
$powershell = (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -replace '\\', '/'
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[print]', 'slicerName=Fake slicer', "slicerProgram=$powershell", 'format=3mf',
  "slicerArguments=-NoProfile, -ExecutionPolicy, Bypass, -File, $($slicerScript -replace '\\', '/')")

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'plate.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"}
]
'@
$made = & $Cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

# The box with its top as the print bed face, then its bottom unstitched:
# Plate (F1.b0) is a surface body of the other five faces that keeps the
# face (mitcad#737).
$surfaceScript = Join-Path $work 'surface.json'
$surface = Join-Path $work 'surface.mitcad'
Set-Content -Encoding ascii $surfaceScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "set_print_bed_face", "uid": "F1.b0", "face": "F1:top"},
  {"cmd": "add_feature", "def": {"type": "unstitch", "body": "F1.b0", "faces": ["F1:bottom"]}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"}
]
'@
$made = & $Cli run $surfaceScript --save $surface
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $surface)) { Ui-Fail "mitcad-cli surface: $($made -join ' / ')" }
$kept = (Get-Content -Raw $surface | ConvertFrom-Json).bodies | Where-Object { $_.uid -eq 'F1.b0' }
if ($kept.print_bed_face -ne 'F1:top') { Ui-Fail "the surface body's print bed face in the file: $($kept.print_bed_face)" }

# Pick-Places: Offset Face started (F7) and cancelled, for the log's pick
# places of the faces.
function Pick-Places {
  Ui-Mark
  Ui-Step 'offset face (F7)'             { Ui-Key 'F7' }
  Ui-ExpectNew 'Pick places:' 'where the faces are picked'
  Ui-Step 'cancel it (Esc)'              { Ui-Key 'Escape' }
  Ui-ExpectNew 'Command Offset Face cancelled' 'cancelled'
}
# Right-Click "face": a right click on the box's face where it is picked.
function Right-Click([string]$Face) {
  Pick-Places
  $at = Ui-PickAt "face F1.b0/$Face at"
  Ui-Mark
  Ui-Step "right-click $Face"            { Ui-Click $at[0] $at[1] 3 }
  Ui-ExpectNew "Selected: 1 face [face $Face of F1.b0" "$Face selected"
  Ui-ExpectNew 'Context menu: ' 'the context menu opened'
}
# Side-Face: $script:side, a side of the box that the view picks (side0 to
# side3, as the view is turned), from fresh pick places.
function Side-Face {
  Pick-Places
  $lines = Ui-LogLines
  $start = 0
  for ($i = $lines.Count - 1; $i -ge 0; $i--) { if ($lines[$i].Contains('Pick places:')) { $start = $i; break } }
  $script:side = $lines[$start..($lines.Count - 1)] | ForEach-Object {
    if ($_ -match 'Pick face F1\.b0/(F1:side[0-3]) at ') { $Matches[1] } } | Select-Object -First 1
  if (-not $script:side) { Ui-Fail 'no side of the box is picked in the view' }
}
# Expect-NoNew "text" "description": no such line since the mark.
function Expect-NoNew([string]$Text, [string]$Description) {
  if ((Ui-LogLines | Select-Object -Skip $script:UiMark) | Where-Object { $_.Contains($Text) }) {
    Ui-Fail $Description
  }
  Write-Host "ok   $Description"
}
# Menu-Entries: the entries of the context menu logged last.
function Menu-Entries { return @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ') }
# Expect-Entry "entry" $true|$false: whether the context menu has it.
function Expect-Entry([string]$Entry, [bool]$Present) {
  $entries = Menu-Entries
  if (($entries -contains $Entry) -ne $Present) {
    Ui-Fail "'$Entry' $(if ($Present) { 'is not' } else { 'is' }) in the context menu: $($entries -join ' | ')"
  }
  Write-Host "ok   the context menu $(if ($Present) { 'has' } else { 'has no' }) $Entry"
}
# Menu-Choose "entry": the entry chosen with the keys, as the menu opens
# with nothing active.
function Menu-Choose([string]$Entry) {
  $entries = Menu-Entries
  $index = [array]::IndexOf($entries, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the context menu: $($entries -join ' | ')" }
  Ui-Step $Entry {
    for ($i = 0; $i -le $index; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
}
# Slicer-Ran: waits for the fake slicer's arguments (PowerShell takes a
# while to start).
function Slicer-Ran {
  for ($i = 0; $i -lt 100; $i++) {
    if (Test-Path $slicerArgs) { return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail 'the slicer was not started'
}
# Send: 3D Print (F8), then Send (Enter).
function Send {
  Remove-Item -Force $slicerArgs -ErrorAction SilentlyContinue
  Ui-Mark
  Ui-Step '3D Print (F8)'                { Ui-Key 'F8' }
  Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
  Ui-Step 'send (Enter)'                 { Ui-Key 'Return' }
}

Ui-StartApp 'print-bed' @('--open', $design)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'

Write-Host '--- 1. Set as Print Bed Face on the top'
Right-Click 'F1:top'
Expect-Entry 'Set as Print Bed Face' $true
Expect-Entry 'Clear Print Bed Face' $false
Ui-Mark
Menu-Choose 'Set as Print Bed Face'
Ui-ExpectNew 'Print bed face of Plate: F1:top' 'the top is the print bed face'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
$saved = Get-Content -Raw $design | ConvertFrom-Json
if ($saved.bodies[0].print_bed_face -ne 'F1:top') { Ui-Fail "the file's print bed face: $($saved.bodies[0].print_bed_face)" }
Write-Host 'ok   the file keeps it with the body'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 2. Another face''s menu shows it'
Side-Face
Right-Click $script:side
Ui-ExpectNew 'Print bed face shown: F1.b0/F1:top' 'the top highlighted while the menu is open'
Expect-Entry 'Set as Print Bed Face' $true
Expect-Entry 'Clear Print Bed Face' $false
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 3. Clear Print Bed Face, Undo'
Right-Click 'F1:top'
Expect-Entry 'Clear Print Bed Face' $true
Expect-Entry 'Set as Print Bed Face' $false
Ui-Mark
Menu-Choose 'Clear Print Bed Face'
Ui-ExpectNew 'Print bed face of Plate: none' 'cleared'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Clear Print Bed Face of Plate' 'one undo brings it back'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }

Write-Host '--- 4. 3D Print turns the box over onto its top'
Send
Ui-ExpectNew ('3D Print bed: Plate on F1:top, plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | ' +
  '0.000 -1.000 0.000 138.000 | 0.000 0.000 -1.000 10.000') 'turned over, the top on the bed, centred on the plate' 30
Slicer-Ran
if (-not (Test-Path (Join-Path $printTemp 'mitcad-print\plate\plate.3mf'))) { Ui-Fail 'no 3MF file' }
Write-Host 'ok   the 3MF file went to the slicer'

Write-Host '--- 5. Print Analysis starts from it'
Ui-Mark
Ui-Step 'print analysis (F9)'            { Ui-Key 'F9' }
Ui-ExpectNew 'Print Analysis bed face: F1.b0/F1:top' 'the analysis starts with the top as its bed face'
Ui-Step 'cancel it (Esc)'                { Ui-Key 'Escape' }
Ui-StopApp

Write-Host '--- 6. A print bed face that is gone'
$gone = Join-Path $work 'gone.mitcad'
$text = [IO.File]::ReadAllText($design).Replace('"print_bed_face": "F1:top"', '"print_bed_face": "F9:top"')
if (-not $text.Contains('"F9:top"')) { Ui-Fail 'the saved file has no print bed face to replace' }
[IO.File]::WriteAllText($gone, $text)
Ui-StartApp 'print-bed-gone' @('--open', $gone)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'
Send
Ui-ExpectNew ('3D Print bed: Plate on F1:bottom (automatic), plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | ' +
  '0.000 1.000 0.000 118.000 | 0.000 0.000 1.000 0.000; Plate: the print bed face is gone') `
  'laid on its bottom by the automatic orientation, centred, the face reported' 30
Ui-ExpectNew '3D Print last line: ' 'the status line'
Expect-NoNew 'printed as in the design' 'the status line does not say it is printed as in the design'
Slicer-Ran

Write-Host '--- 6b. The gone face cleared from another face''s menu'
Side-Face
Right-Click $script:side
Expect-Entry 'Clear Print Bed Face' $true
Expect-Entry 'Set as Print Bed Face' $true
Ui-Mark
Menu-Choose 'Clear Print Bed Face'
Ui-ExpectNew 'Print bed face of Plate: none' 'the gone face cleared'
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Send
Ui-ExpectNew '3D Print bed: Plate on F1:bottom (automatic), plate 128.000,128.000' 'laid on its bottom by the automatic orientation' 30
Slicer-Ran
Expect-NoNew 'the print bed face is gone' '3D Print warns no more'
Ui-StopApp

Write-Host '--- 7. A surface body that keeps a print bed face'
Ui-StartApp 'print-bed-surface' @('--open', $surface)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the surface body shown'
Right-Click 'F1:top'
Expect-Entry 'Clear Print Bed Face' $true
Expect-Entry 'Set as Print Bed Face' $false
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Side-Face
Right-Click $script:side
Expect-Entry 'Clear Print Bed Face' $true
Expect-Entry 'Set as Print Bed Face' $false
Expect-NoNew 'Print bed face shown:' 'no print bed face shown on a surface body'
Ui-Mark
Menu-Choose 'Clear Print Bed Face'
Ui-ExpectNew 'Print bed face of Plate: none' 'the surface body''s face cleared'
# Without one, a planar face of the surface body offers none and says why
# (mitcad#1182).
Ui-Step 'clear the selection (Esc)'      { Ui-Key 'Escape' }
Right-Click 'F1:top'
Expect-Entry 'Set as Print Bed Face' $false
Expect-Entry 'Clear Print Bed Face' $false
Ui-ExpectNew 'Print bed face hint: Plate is a surface body, not a closed solid: it is not printed and has no print bed face' 'the status line says why the surface body has no print bed face'
Ui-Step 'close the menu (Esc)'           { Ui-Key 'Escape' }
Ui-StopApp

Write-Host '--- 8. Send to Slicer''s Bed Face (mitcad#1109)'
$fresh = Join-Path $work 'fresh.mitcad'
$made = & $Cli run $modelScript --save $fresh
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $fresh)) { Ui-Fail "mitcad-cli fresh: $($made -join ' / ')" }
# The body's print bed face in a design file, empty without one.
function Stored-BedFace([string]$Path) {
  $body = (Get-Content -Raw $Path | ConvertFrom-Json).bodies | Where-Object { $_.uid -eq 'F1.b0' }
  return "$($body.print_bed_face)"
}
Ui-StartApp 'print-bed-face-row' @('--open', $fresh)
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'
Pick-Places
Remove-Item -Force $slicerArgs -ErrorAction SilentlyContinue
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Panel Send to Slicer input format at ' 'the panel laid out'
Ui-ExpectNew '3D Print bed face: Plate: automatic; Plate F1:bottom (automatic)' `
  'choosing the body fills Bed Face with the automatic orientation''s face'
Ui-Step 'the Bed Face row'               { Ui-ClickLogged 'Panel Send to Slicer input print_bed' }
Ui-ExpectNew 'Send to Slicer marks: face F1.b0/F1:bottom' 'the bottom highlighted while the panel is open'
Expect-NoNew 'picks not drawn beside the marks: Bodies' 'the body''s own highlight kept beside it'
if (Stored-BedFace $fresh) { Ui-Fail "the automatic face was stored: $(Stored-BedFace $fresh)" }
Write-Host 'ok   the automatic face is not stored'
Ui-Mark
Ui-Step 'pick the top in Bed Face'       { Ui-ClickPick 'face F1.b0/F1:top at' }
Ui-ExpectNew '3D Print bed face set: Plate F1:top' 'the top made the body''s print bed face'
Ui-ExpectNew '3D Print bed face: Plate: print bed face; Plate F1:top' 'Bed Face shows it'
Ui-ExpectNew 'Send to Slicer marks: face F1.b0/F1:top' 'the highlight follows'
Ui-Mark
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew ('3D Print bed: Plate on F1:top, plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | ' +
  '0.000 -1.000 0.000 138.000 | 0.000 0.000 -1.000 10.000') 'sent turned over onto the top' 30
Slicer-Ran
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Saved ' 'saved'
if ((Stored-BedFace $fresh) -ne 'F1:top') { Ui-Fail "the file's print bed face: $(Stored-BedFace $fresh)" }
Write-Host 'ok   the top stored as the body''s print bed face'
Ui-Mark
Ui-Step 'undo (Ctrl+Z)'                  { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Set Print Bed Face of Plate' 'one undo step'

Write-Host '--- 9. The bed plan in the background (mitcad#1121)'
# The panel's bed plan made 2.5 s longer; Enter posted at once, while it
# runs (not a Ui-Step: that waits for the job), sends once it is done.
$env:MITCAD_TEST_JOB_SLEEP_MS = 'bed plan=2500'
Ui-StartApp 'print-bed-plan' @('--open', $fresh)
Remove-Item Env:MITCAD_TEST_JOB_SLEEP_MS -ErrorAction SilentlyContinue
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the box shown'
Remove-Item -Force $slicerArgs -ErrorAction SilentlyContinue
Ui-Mark
Ui-Step '3D Print (F8)'                  { Ui-Key 'F8' }
Ui-ExpectNew 'Computing bed plan started' 'the bed plan runs as a job of its own'
# "Planning" and its ellipsis: the script stays ASCII.
Ui-ExpectNew '3D Print bed face: Planning' 'Bed Face says Planning meanwhile'
if (@(Ui-NewLines | Where-Object { $_ -match 'Computing bed plan done in [0-9]+ ms' }).Count -gt 0) {
  Ui-Fail 'the plan was done before Enter was typed'
}
Ui-Key 'Return'
Ui-ExpectNew '3D Print: Send waited for the bed plan (' 'Enter waited for the plan' 30
$waited = @(Ui-NewLines | Where-Object { $_.Contains('3D Print: Send waited for the bed plan (') }).Count
if ($waited -ne 1) { Ui-Fail "the wait was logged $waited times" }
Ui-ExpectNew '3D Print bed face: Plate: print bed face; Plate F1:top' 'the plan done: Bed Face shows the top'
Ui-ExpectNew '3D Print bed: Plate on F1:top, plate 128.000,128.000' 'then sent, on the top' 30
Slicer-Ran
# No progress dialog while the plan ran: nothing took the Enter.
foreach ($line in @(Ui-NewLines)) {
  if ($line.Contains('3D Print: Send waited for the bed plan')) { break }
  if ($line.Contains('Progress dialog shown')) { Ui-Fail "a progress dialog showed while the bed plan ran: $line" }
}
Expect-NoNew 'Progress dialog shown: bed plan' 'the bed plan shows no progress dialog'
Expect-NoNew 'The document was read while a job computes it' 'nothing read the design during the plan'
Write-Host 'ok   no progress dialog while the plan ran'

Ui-Finish 'UI Windows print bed test'
