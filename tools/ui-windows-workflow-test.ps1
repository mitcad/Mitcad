# SPDX-License-Identifier: MIT
# Input-driven Windows UI test for the build VM: the basic workflows through
# the real UI, with mouse and keyboard messages posted to the window
# (tools/ui-windows-lib.ps1 says why not SendInput or UI Automation):
#   1. Create Sketch on XY (a click on the plane), a 40 x 25 mm rectangle
#      (a click at the origin, the sizes typed), Finish Sketch (Ctrl+Enter).
#   2. Extrude (E) 20 mm: a 20000 mm3 body.
#   3. Fillet (F) the edges of the top face (a click on it), 2 mm.
#   4. Autosave, every 2 s for the test (MITCAD_TEST_AUTOSAVE_SECONDS): the
#      changed design is in the test's recovery folder (QSaveFile and
#      QLockFile on Windows).
#   5. Recovery: the app killed and started again offers the design; Restore
#      brings it back, and the new session takes over its files.
#   6. Save As (Ctrl+Shift+S) through Qt's file dialog, which removes the
#      autosaved files; New (Ctrl+N); Open (Ctrl+O) the file again: the
#      body recomputed with the saved volume (Physical Properties), so the
#      recovered design was the filleted one.
#   7. Export a STEP file through the Export dialog, and a web page (mitcad#423:
#      one .html file with the design's data, its size in the log); New;
#      Import the STEP file: a base feature of the same volume; undo (Ctrl+Z).
#   8. Version history (P12d, mitcad#89): the saved design opened with a
#      change, Save to Project makes its folder a Local project (the picker's New
#      Project with the folder offered; the author typed there, as git's
#      configuration is the test's own, without a user): the design as
#      saved is the first version, the open one the second; Ctrl+S records
#      a third; git's log has them where git is installed.
#   9. Version History (P12e, Ctrl+Shift+H): the three versions with what
#      each changed, compared; Restore of the second (the changed one)
#      records it as a fourth version and opens it.
#  10. 3D Print (mitcad#13, its panel mitcad#234) to a fake slicer (a
#      PowerShell script that writes its arguments): the body highlighted
#      in the panel, sent as an STL file, then as a 3MF file (the format
#      chosen with the wheel; one object, the part named after the body,
#      closed, of the body's volume), each in one start; the folder holds
#      only the last send.
#  11. Cloud project (P12 remote, mitcad#89, where git is installed):
#      Project Settings shares the project to a bare repository in a folder
#      (its path typed into the Cloud section's address) and sends the
#      versions; opened again with a change, the remote is checked, Ctrl+S
#      records a version and sends it at once; Sync (Ctrl+Alt+Y) finds the
#      project up to date.
#  12. Helix and Hole (mitcad#27) on a design mitcad-cli makes: Helix (a
#      shortcut of the test's) turns a 2 mm square about the Z axis (a
#      click on it), 3 turns of a typed pitch, of the volume by Pappus'
#      rule; a counterdrilled, tapered hole as the FreeCAD import writes
#      one opens from the timeline (a double-click), and a typed taper keeps
#      its counterdrill; edited to a Clearance hole for M6 (mitcad#903) the
#      size fills its diameter and counterdrill, and a typed depth stays
#      when the size changes; a typed Countersink Diameter is marked and
#      its Reset to size makes it follow the size again (mitcad#980).
#  13. Joints (mitcad#55) on a design mitcad-cli makes: a pin on a revolute
#      joint dragged in the view turns about its joint (one undo step); Joint
#      (J) puts a cap's top face onto a plate's, flipped, the preview showing
#      the cap where it goes.
#  14. Extrude's profile clicks (mitcad#199) in a sketch of a placed
#      component: a click on the newest profile takes it out instead of
#      listing it twice; clicks on nested squares' regions toggle them, and
#      the bodies have the odd/even area that is left.
#  15. 3D Print's panel (mitcad#234) on a design mitcad-cli makes, a plate,
#      a cube and a surface body: with nothing selected the plate and the
#      cube are in Bodies and highlighted, the surface left out; a click on
#      the cube takes it and its highlight out, a click on the surface picks
#      nothing; Send writes the plate's file alone.
# Commands without a default shortcut get one in the test's settings
# (popups such as the command search close at once in session 0).
# Mitcad runs with Mesa's software OpenGL (MITCAD_MESA_DIR) in session 0
# over SSH, as tools/ui-windows-test.ps1 does.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-workflow-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-workflow'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{
  'sketch.create' = 'F7'; 'inspect.properties' = 'F8'; 'file.export' = 'F9'; 'file.import' = 'F11'
  'file.save_to_project' = 'F12'; 'make.print3d' = 'Shift+F9'; 'file.project_settings' = 'Shift+F11'
  'solid.helix' = 'Shift+F7' } -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# 3D Print (part 10): the app's temporary folder of the test's own, and a
# fake slicer in the settings.
$printTemp = Join-Path $script:UiOut 'tmp'
Remove-Item -Recurse -Force $printTemp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $printTemp | Out-Null
$env:TMP = $printTemp
$env:TEMP = $printTemp
$slicerArgs = Join-Path $script:UiOut 'slicer-args.txt'
# Not the arguments of an earlier run's last print, which the first
# Slicer-Arguments would take before the slicer wrote its own (mitcad#271).
Remove-Item -Force $slicerArgs, "$slicerArgs.part" -ErrorAction SilentlyContinue
$slicerScript = Join-Path $script:UiOut 'fake-slicer.ps1'
Set-Content -Encoding ascii $slicerScript (@'
[IO.File]::WriteAllLines('ARGS.part', [string[]]$args)
Move-Item -Force 'ARGS.part' 'ARGS'
'@ -creplace 'ARGS', $slicerArgs)
$powershell = (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -replace '\\', '/'
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[print]', 'slicerName=Fake slicer', "slicerProgram=$powershell", 'format=stl',
  "slicerArguments=-NoProfile, -ExecutionPolicy, Bypass, -File, $($slicerScript -replace '\\', '/')")
# The app logs paths with forward slashes.
$workLogged = $work -replace '\\', '/'
$invariant = [Globalization.CultureInfo]::InvariantCulture

# A path typed into a file dialog's focused field, then Enter.
function Type-Into-Dialog([string]$Title, [string]$Path) {
  Ui-FocusDialog $Title
  Ui-Key 'ctrl+a'
  Ui-Type $Path
  # The characters handled before Enter (mitcad#870: a fixed 300 ms before).
  Ui-Sync
  Ui-Key 'Return'
  Ui-FocusMain
}

Write-Host '--- Sketch, extrude, fillet'
$env:MITCAD_TEST_AUTOSAVE_SECONDS = '2'
Ui-StartApp 'workflow'
Ui-Step 'create sketch on XY (F7)'       { Ui-CreateSketch 'F7' 'xy' }
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
Ui-ExpectNew 'Added extrude (New Body), distance 20' 'extrusion built'
Ui-ExpectVolume 'New body \S+ \(F2\.b0\): volume ([0-9.]+) mm3' 20000 'the extruded body'

Ui-Mark
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
Ui-ExpectNew 'Pick places:' 'Fillet logs where its picks are'
Ui-Step 'pick the top face'              { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectNew 'Fillet Edges: 1 face' 'the top face picked'
Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
Ui-ExpectNew 'Added fillet on 4 edge(s)' "fillet on the face's 4 edges"
Ui-ExpectNew 'Body Body1 (F2.b0): volume 20000.000 -> ' 'the fillet took material off'
# Four edges of 2 * 40 + 2 * 25 mm lose (1 - pi/4) r^2 each per length;
# where two meet, the removed corners overlap by r^3 (5/3 - pi/2).
$r = 2.0
$expected = 20000 - (1 - [Math]::PI / 4) * $r * $r * 130 + 4 * [Math]::Pow($r, 3) * (5.0 / 3 - [Math]::PI / 2)
Ui-ExpectVolume 'Body Body1 \(F2\.b0\): volume 20000\.000 -> ([0-9.]+) mm3' $expected 'the filleted body'
$filleted = [double]::Parse((Ui-LastMatch 'Body Body1 \(F2\.b0\): volume 20000\.000 -> ([0-9.]+) mm3'), $invariant)

Write-Host '--- Autosave'
Ui-ExpectLog 'Autosaved Untitled: ' 'the changed design was autosaved'
# Within one interval more the last change is written, and nothing after
# it: the autosave with the fillet looked for until then (mitcad#870: one
# look after a fixed 2.5 s before); the checks below say what is wrong if
# it did not come.
$clock = [Diagnostics.Stopwatch]::StartNew()
do {
  try {
    $metadata = @(Get-ChildItem $script:UiAutosave -Filter '*.json')
    if ($metadata.Count -eq 1) {
      $about = Get-Content -Raw $metadata[0].FullName | ConvertFrom-Json
      $autosaved = Join-Path $script:UiAutosave $about.project
      if ((Test-Path $autosaved) -and (Get-Item $autosaved).Length -eq $about.size) {
        $features = (Get-Content -Raw $autosaved | ConvertFrom-Json).features | ForEach-Object { $_.type }
        if (($features -join ',') -eq 'sketch,extrude,fillet') { break }
      }
    }
  } catch {
    # A file being written: looked at again.
  }
  Start-Sleep -Milliseconds 100
} while ($clock.ElapsedMilliseconds -lt 2500)
$metadata = @(Get-ChildItem $script:UiAutosave -Filter '*.json')
if ($metadata.Count -ne 1) { Ui-Fail "$($metadata.Count) autosave sessions in $($script:UiAutosave)" }
$about = Get-Content -Raw $metadata[0].FullName | ConvertFrom-Json
if ($about.format -ne 'mitcad-autosave' -or $about.version -ne 2 -or $about.document -ne 'Untitled') {
  Ui-Fail "autosave metadata: $($about | ConvertTo-Json -Compress)"
}
$autosaved = Join-Path $script:UiAutosave $about.project
if (-not (Test-Path $autosaved) -or (Get-Item $autosaved).Length -ne $about.size) {
  Ui-Fail "the autosaved project file is not the size its metadata gives ($($about.size))"
}
$features = (Get-Content -Raw $autosaved | ConvertFrom-Json).features | ForEach-Object { $_.type }
if (($features -join ',') -ne 'sketch,extrude,fillet') { Ui-Fail "autosaved features: $($features -join ',')" }
if (-not (Test-Path (Join-Path $script:UiAutosave "$($about.session).lock"))) { Ui-Fail 'no lock of the session' }
Write-Host 'ok   the project file with the fillet, its metadata and the lock'

Write-Host '--- Recovery after a crash'
# Killed, the app leaves its session; started again, it finds the lock stale
# (the process is gone) and offers the design.
Ui-StopApp
Ui-StartApp 'recovered' -Recovery
Ui-ExpectLog 'Recovery: 1 document(s) found' 'the killed session is offered'
Ui-ExpectLog 'Recoverable Untitled: never saved, autosaved ' 'the untitled design'
Ui-FocusDialog '^Recover Unsaved Work$'
Ui-Mark
Ui-Step 'restore (Enter)'                { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Recovered Untitled from autosave of ' 'restored'
Ui-ExpectNew 'Bodies shown: F2.b0' 'the filleted body recomputed and shown'
Ui-ExpectNew "Recovered session $($about.session) removed" "this session wrote it and removed the killed one's files"
$metadata = @(Get-ChildItem $script:UiAutosave -Filter '*.json')
if ($metadata.Count -ne 1) { Ui-Fail "$($metadata.Count) autosave sessions in $($script:UiAutosave)" }
$session = (Get-Content -Raw $metadata[0].FullName | ConvertFrom-Json).session
if ($session -eq $about.session) { Ui-Fail 'the killed session is still there' }
Write-Host "ok   the design is in this session's files"

Write-Host '--- Save, new, open'
$project = Join-Path $work 'block.mitcad'
Ui-Mark
Ui-Step 'save as (Ctrl+Shift+S)'         { Ui-Key 'ctrl+shift+s' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Save As$' $project }
Ui-ExpectNew "Saved $workLogged/block.mitcad" 'saved through the dialog'
if (-not (Test-Path $project)) { Ui-Fail 'no project file' }
Ui-ExpectNew 'Autosave removed for Untitled' 'saving removed the autosaved files'
$left = @(Get-ChildItem $script:UiAutosave | ForEach-Object { $_.Name })
if ($left.Count -ne 1 -or $left[0] -ne "$session.lock") { Ui-Fail "left in the recovery folder: $($left -join ', ')" }
Write-Host 'ok   only the lock of the session is left'
Ui-Step 'new (Ctrl+N)'                   { Ui-Key 'ctrl+n' }
Ui-ExpectNew 'New document' 'an empty document'
Ui-Mark
Ui-Step 'open (Ctrl+O)'                  { Ui-Key 'ctrl+o' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Open$' $project }
Ui-ExpectNew "Opened $workLogged/block.mitcad" 'opened through the dialog'
Ui-ExpectNew 'Bodies shown: F2.b0' 'the body recomputed and shown'
Ui-Mark
Ui-Step 'physical properties (F8)'       { Ui-Key 'F8' }
Ui-ExpectNew 'Physical properties: Body1 volume' 'the properties dialog opened'
$reopened = [double]::Parse((Ui-LastMatch 'Physical properties: Body1 volume ([0-9.]+)'), $invariant)
if ([Math]::Abs($reopened - $filleted) -gt 1e-3) { Ui-Fail "reopened volume $reopened, saved $filleted" }
Write-Host "ok   the reopened body has the saved volume: $reopened mm3"
Ui-Step 'close the dialog (Esc)'         { Ui-FocusDialog '^Properties$'; Ui-Key 'Escape'; Ui-FocusMain }

Write-Host '--- Export and import STEP'
$step = Join-Path $work 'block.step'
Ui-Mark
Ui-Step 'export (F9)'                    { Ui-Key 'F9' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Export$' $step }
Ui-ExpectNew "Exported ${step}: step, 1 body(ies)" 'the body written as STEP'
if (-not (Test-Path $step)) { Ui-Fail 'no STEP file' }
Write-Host '--- Export a web page (mitcad#423)'
$html = Join-Path $work 'block.html'
Ui-Mark
Ui-Step 'export (F9)'                    { Ui-Key 'F9' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Export$' $html }
Ui-ExpectNew "Exported ${html}: html, 1 body(ies), " 'the body written as a web page'
if (-not (Test-Path $html)) { Ui-Fail 'no web page' }
$size = (Get-Item $html).Length
Ui-ExpectNew "Exported ${html}: html, 1 body(ies), $size bytes" 'the log says the page''s size'
if (-not (Select-String -Path $html -SimpleMatch 'id="mitcad-data"' -Quiet)) { Ui-Fail 'no design data in the page' }
if (Select-String -Path $html -SimpleMatch '<!--MITCAD:' -Quiet) { Ui-Fail 'a placeholder left in the page' }
Write-Host 'ok   the page holds the design''s data'
Ui-Step 'new (Ctrl+N)'                   { Ui-Key 'ctrl+n' }
Ui-ExpectNew 'New document' 'an empty document'
Ui-Mark
Ui-Step 'import (F11)'                   { Ui-Key 'F11' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Import$' $step }
Ui-ExpectNew 'Imported block.step as' 'the STEP came in as a base feature'
$imported = Ui-LastMatch 'New body \S+ \(F\d+\.b0\): volume ([0-9.]+) mm3'
if ($null -eq $imported) { Ui-Fail 'no volume of the imported body' }
$imported = [double]::Parse($imported, $invariant)
if ([Math]::Abs($imported - $filleted) -gt 1e-6 * $filleted) { Ui-Fail "imported volume $imported, exported $filleted" }
Write-Host "ok   the imported body has the exported volume: $imported mm3"
Ui-Step 'undo the import (Ctrl+Z)'       { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Import block.step' 'the import is one undo step'

Write-Host '--- Version history: the folder made a project, two saves, two versions'
# Git's configuration of the test's own, without a user: the author is
# asked once and kept in the test's settings (never the machine's identity).
$gitConfig = Join-Path $script:UiOut 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$env:MITCAD_PROJECTS_DIR = Join-Path $script:UiOut 'projects'
Ui-StartApp 'versions' @('--open', $project, '--set', 'd3=25')
Ui-ExpectLog "Opened $workLogged/block.mitcad" 'the saved design opened, changed'
Ui-ExpectLog 'Version status: none' 'outside projects: no version history'
Ui-ExpectLog 'Project indicator: not in a project' 'the indicator says so'
Ui-Mark
Ui-Step 'save to project (F12)'          { Ui-Key 'F12' }
Ui-ExpectNew 'Project picker Save to Project: a place, reading the projects' 'the project picker'
Ui-Step 'a new project (Alt+P)'          { Ui-FocusDialog '^Save to Project$'; Ui-Key 'alt+p' }
Ui-ExpectNew "New Project dialog: $workLogged" "the design's own folder offered"
Ui-ExpectNew "New Project dialog: folder ${workLogged}: designs" 'a folder of designs'
# git has no user and Mitcad no default author: the author's fields are
# empty, and the name has the focus.
Ui-FocusDialog '^Save to Project$'
Ui-Step 'type the name'                  { Ui-Type 'Windows Tester' }
Ui-Step 'the email (Tab)'                { Ui-Key 'Tab' }
Ui-Step 'type the email'                 { Ui-Type 'windows@example.invalid' }
Ui-Step 'create (Enter)'                 { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew "New Project: created ${workLogged}: version " 'the folder is a Local project' 20
Ui-ExpectNew 'Version author: Windows Tester <windows@example.invalid> (git)' "the project's author"
Ui-ExpectNew 'Version recorded: block.mitcad ' 'the open design is the second version' 20
Ui-ExpectNew 'Version status: files, main, v2' 'the first version is the design as saved'
Ui-ExpectNew 'Project indicator: files, local, v2' 'the indicator shows it'
Ui-ExpectNew 'Version preview saved: ' "the version's preview"
if (@(Get-ChildItem $script:UiThumbnails -Filter '*.png' -ErrorAction SilentlyContinue).Count -ne 1) {
  Ui-Fail "no preview in $($script:UiThumbnails)"
}
Write-Host "ok   the preview is in the test's own folder"
Ui-Mark
Ui-Step 'undo the change of d3 (Ctrl+Z)' { Ui-Key 'ctrl+z' }
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Version recorded: block.mitcad ' 'Ctrl+S recorded the third version'
Ui-ExpectNew 'Version status: files, main, v3' 'three versions of the design'
$git = Get-Command git -ErrorAction SilentlyContinue
if ($git) {
  $log = & $git.Source -C $work log --format='%s|%an|%ae' 2>&1
  $expected = @('Save block.mitcad: Undo Change d3|Windows Tester|windows@example.invalid',
                'Save block.mitcad: Change d3|Windows Tester|windows@example.invalid',
                'Create project files|Windows Tester|windows@example.invalid')
  if (($log -join "`n") -ne ($expected -join "`n")) { Ui-Fail "git log: $($log -join ' / ')" }
  Write-Host "ok   git's log has the three versions"
  $status = & $git.Source -C $work status --porcelain 2>&1
  # The STEP and web page files of the exports are the user's, not a version's.
  if (($status -join "`n") -ne "?? block.html`n?? block.step") { Ui-Fail "git status: $($status -join ' / ')" }
  Write-Host 'ok   git status shows only the file Mitcad does not record'
} else {
  Write-Host 'note: git is not installed; the versions are checked through the log only'
}

Write-Host '--- Version History: the three versions compared, the second restored'
Ui-Mark
Ui-Step 'version history (Ctrl+Shift+H)' { Ui-Key 'ctrl+shift+h' }
Ui-ExpectNew 'Version History: 3 versions of block.mitcad: v3 ' 'the three versions, newest first'
Ui-ExpectNew 'Version History changes: v3 d3 25 mm -> 20 mm' 'what the third version changed'
Ui-ExpectNew 'Version History selected v3 (' 'the latest selected'
Ui-ExpectNew 'Version History compare v3 with v2: ' 'compared with the second'
Ui-ExpectNew 'd3 (Extrude1 distance): 25 mm -> 20 mm' 'the change of d3'
Ui-FocusDialog '^Version History - block\.mitcad$'
Ui-Mark
Ui-Step 'the second version (Down)'      { Ui-Key 'Down' }
Ui-ExpectNew 'Version History selected v2 (' 'v2 selected'
Ui-Step 'restore (Alt+R)'                { Ui-Key 'alt+r' }
Ui-ExpectNew 'Restore dialog: v2 (' 'asked'
Ui-Step 'restore (Enter)'                { Ui-FocusDialog '^Restore Version$'; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Version restored: block.mitcad v2 (' 'the second version restored as a new one'
Ui-ExpectNew "Opened $workLogged/block.mitcad" 'the restored design opened'
Ui-ExpectNew 'Version status: files, main, v4' 'four versions of the design'
if ($git) {
  $log = & $git.Source -C $work log -1 --format='%s|%an' 2>&1
  if ($log -notmatch '^Restore v2 of block\.mitcad \([0-9a-f]{7}\)\|Windows Tester$') { Ui-Fail "git log: $log" }
  Write-Host "ok   git's log has the restore"
}

Write-Host '--- 3D Print to a fake slicer'
$printFolder = [IO.Path]::GetFullPath((Join-Path $printTemp 'mitcad-print\block'))
# The fake slicer's arguments, once it ran (PowerShell takes a while to start).
function Slicer-Arguments {
  # Looked for every 50 ms (200 ms before, mitcad#870), up to 20 s.
  $clock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if (Test-Path $slicerArgs) { return @(Get-Content $slicerArgs | ForEach-Object { [IO.Path]::GetFullPath($_) }) }
    Start-Sleep -Milliseconds 50
  } while ($clock.ElapsedMilliseconds -lt 20000)
  Ui-Fail 'the slicer was not started'
}
Ui-Mark
Ui-Step '3D print (Shift+F9)'            { Ui-Key 'shift+F9' }
Ui-ExpectNew '3D Print panel: 1 visible: Body1; format stl, refinement high, slicer Fake slicer' 'the panel: the visible body, STL, High, the slicer of the settings'
Ui-ExpectNew 'View body highlights: F' 'the body to send highlighted'
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew '3D Print: 1 bodies as stl (high) to ' 'the body written as STL'
Ui-ExpectNew '3D Print: started ' 'the slicer started with it'
$stl = Join-Path $printFolder 'Body1.stl'
$got = @(Slicer-Arguments)
if ($got.Count -ne 1 -or $got[0] -ne $stl) { Ui-Fail "the slicer's arguments: $($got -join ' | ')" }
if (-not (Test-Path $stl) -or (Get-Item $stl).Length -lt 684) { Ui-Fail "no STL file $stl" }
Write-Host 'ok   one start with the STL file'
Remove-Item -Force $slicerArgs
Ui-Mark
Ui-Step '3D print (Shift+F9)'            { Ui-Key 'shift+F9' }
Ui-ExpectNew '3D Print panel: 1 visible: Body1; format stl' 'the panel, STL remembered'
Ui-ExpectNew 'Panel 3D Print input format at ' 'the panel laid out'
Ui-Step 'format 3MF (wheel)'             { Ui-WheelLogged 'Panel 3D Print input format' -1 }
Ui-ExpectNew '3D Print: Format = 3MF' 'the format 3MF chosen'
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew '3D Print: 1 bodies as 3mf (high) to ' 'the body written as 3MF'
$threeMf = Join-Path $printFolder 'block.3mf'
$got = @(Slicer-Arguments)
if ($got.Count -ne 1 -or $got[0] -ne $threeMf) { Ui-Fail "the slicer's arguments: $($got -join ' | ')" }
$left = @(Get-ChildItem $printFolder | ForEach-Object { $_.Name })
if (($left -join ',') -ne 'block.3mf') { Ui-Fail "the folder was not emptied: $($left -join ', ')" }
Write-Host 'ok   one start with the 3MF file; the STL file of the last send is gone'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($threeMf)
try {
  $reader = New-Object IO.StreamReader ($zip.GetEntry('3D/3dmodel.model').Open())
  [xml]$model = $reader.ReadToEnd()
  $reader.Dispose()
} finally { $zip.Dispose() }
$ns = New-Object Xml.XmlNamespaceManager $model.NameTable
$ns.AddNamespace('m', 'http://schemas.microsoft.com/3dmanufacturing/core/2015/02')
$items = $model.SelectNodes('/m:model/m:build/m:item', $ns)
$assembly = $items[0].GetAttribute('objectid')
$parts = $model.SelectNodes("/m:model/m:resources/m:object[@id='$assembly']/m:components/m:component", $ns)
if ($items.Count -ne 1 -or $parts.Count -ne 1) { Ui-Fail "3MF: $($items.Count) build items, $($parts.Count) components" }
$part = $model.SelectSingleNode("/m:model/m:resources/m:object[@id='$($parts[0].GetAttribute('objectid'))']", $ns)
if ($part.GetAttribute('name') -ne 'Body1') { Ui-Fail "3MF: the part is named '$($part.GetAttribute('name'))'" }
$vertices = @($part.SelectNodes('m:mesh/m:vertices/m:vertex', $ns) | ForEach-Object {
  , @(foreach ($axis in 'x', 'y', 'z') { [double]::Parse($_.GetAttribute($axis), $invariant) }) })
$edges = @{}
$six = 0.0
foreach ($t in $part.SelectNodes('m:mesh/m:triangles/m:triangle', $ns)) {
  $v = @(foreach ($corner in 'v1', 'v2', 'v3') { [int]$t.GetAttribute($corner) })
  $a = $vertices[$v[0]]; $b = $vertices[$v[1]]; $c = $vertices[$v[2]]
  $six += $a[0] * ($b[1] * $c[2] - $b[2] * $c[1]) - $a[1] * ($b[0] * $c[2] - $b[2] * $c[0]) + $a[2] * ($b[0] * $c[1] - $b[1] * $c[0])
  for ($k = 0; $k -lt 3; $k++) { $edges["$($v[$k])-$($v[($k + 1) % 3])"] += 1 }
}
foreach ($edge in $edges.Keys) {
  $ends = $edge -split '-'
  if ($edges[$edge] -ne 1 -or $edges["$($ends[1])-$($ends[0])"] -ne 1) { Ui-Fail "3MF: the mesh is not closed at $edge" }
}
$meshed = $six / 6
# The restored second version's body: the filleted block extruded 25 mm
# instead of 20 (d3), 40 x 25 x 5 mm more.
$body = $filleted + 40 * 25 * 5
if ([Math]::Abs($meshed - $body) -gt 1e-3 * $body) { Ui-Fail "3MF: mesh volume $meshed, the body's $body" }
Write-Host "ok   one object, its part Body1 closed, $([Math]::Round($meshed, 3)) mm3 of $body"

Write-Host '--- Cloud project: shared in Project Settings, a saved version sent, sync'
if ($git) {
  $remote = Join-Path $script:UiOut 'remote.git'
  $remoteLogged = $remote -replace '\\', '/'
  Remove-Item -Recurse -Force $remote -ErrorAction SilentlyContinue
  & $git.Source init -q --bare $remote 2>&1 | Out-Null
  # The project's branch is the remote's main.
  function Expect-Sent([string]$Description) {
    $mine = & $git.Source -C $work rev-parse HEAD 2>&1
    $theirs = & $git.Source --git-dir=$remote rev-parse main 2>&1
    if ("$mine" -ne "$theirs") { Ui-Fail "${Description}: the remote has $theirs, the project $mine" }
    Write-Host "ok   $Description"
  }
  Ui-Mark
  Ui-Step 'project settings (Shift+F11)'   { Ui-Key 'shift+F11' }
  Ui-ExpectNew 'Project Settings dialog: files, local; ' 'Project Settings of the Local project'
  Ui-FocusDialog '^Project Settings - files$'
  Ui-Step 'cloud (Alt+O)'                  { Ui-Key 'alt+o' }
  Ui-ExpectNew 'Project Settings: storage cloud (share)' 'the Cloud section'
  Ui-Step 'the address (Alt+E)'            { Ui-Key 'alt+e' }
  Ui-Step 'type the address'               { Ui-Key 'ctrl+a'; Ui-Type $remote }
  Ui-ExpectNew "Cloud check ${remoteLogged}: " 'the address checked' 30
  Ui-Step 'share (Alt+S)'                  { Ui-Key 'alt+s' }
  Ui-ExpectNew "Project Settings: shared files to ${remoteLogged}: versions sent" 'shared, the versions sent' 30
  Ui-Step 'close (Esc)'                    { Ui-Key 'Escape' }
  Ui-FocusMain
  Ui-ExpectNew 'Remote status: synced' 'synced' 30
  Ui-ExpectNew 'Project indicator: files, cloud, synced' 'the indicator: Cloud, synced' 30
  Ui-ExpectNew 'Edit lock taken: block.mitcad' 'the design of the Cloud project has its edit lock (mitcad#89)' 30
  Expect-Sent 'the remote has the versions'
  # Killed, the session leaves its edit lock on the remote: the same author
  # in the next session is asked, and takes it over.
  Ui-StartApp 'remote' @('--open', $project, '--set', 'd3=30')
  Ui-ExpectLog 'Edit lock dialog: You have this design open elsewhere' 'the killed session still holds the lock' 30
  Ui-FocusDialog '^Edit Lock$'
  Ui-Step 'take over (Alt+T)'              { Ui-Key 'alt+t' }
  Ui-FocusMain
  Ui-ExpectLog 'Edit lock taken: block.mitcad from ' 'taken over' 30
  Ui-ExpectLog 'Read-only: off' 'editable again' 30
  Ui-ExpectLog 'Remote check: ahead 0, behind 0' 'the remote checked at opening' 30
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Version recorded: block.mitcad ' 'a version'
  Ui-ExpectNew 'Remote push: sent 1 version(s) to origin/main' 'sent at once' 30
  Ui-ExpectNew 'Remote status: synced' 'synced again' 30
  Expect-Sent 'the remote has the new version'
  Ui-Mark
  Ui-Step 'sync (Ctrl+Alt+Y)'              { Ui-Key 'ctrl+alt+y' }
  Ui-ExpectNew 'Sync done: Up to date with origin/main.' 'Sync: up to date' 30
} else {
  Write-Host 'note: git is not installed; no remote repository'
}

Write-Host '--- Helix and a counterdrilled, tapered hole (mitcad#27)'
# The design: a 60 x 40 x 20 block (Sketch1 F1, Extrude1 F2); a hole at a
# 6.6 mm circle's centre (Plane1 F3 on the top face's plane, Sketch2 F4),
# counterdrilled 11 mm 3 deep at 90 degrees, tapered 2 degrees, through all,
# as the FreeCAD import writes one (Hole1 F5); a 2 mm square on XZ 11 mm
# from the Z axis (Sketch3 F6), the newest profile.
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$drilledScript = Join-Path $work 'drilled.json'
$drilled = Join-Path $work 'drilled.mitcad'
Set-Content -Encoding ascii $drilledScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_circle", "sketch": "F4", "center": [30, 20], "diameter": 6.6},
  {"cmd": "add_feature", "def": {"type": "hole", "placement": {"type": "sketch_points", "sketch": "F4", "points": ["c1"]},
    "diameter": 6.6, "kind": {"type": "counterdrill", "diameter": 11, "depth": 3, "angle": 1.5707963267948966},
    "taper": 0.03490658503988659, "flat": true, "extent": {"type": "through_all"}, "flip": false,
    "participants": ["F2.b0"]}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [10, 0], "width": 2, "height": 2}
]
'@ -creplace 'RECT', 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}')
$made = & $cli run $drilledScript --save $drilled
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $drilled)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Ui-StartApp 'mitcad27' @('--open', $drilled)
Ui-ExpectLog "Opened $workLogged/drilled.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'helix (Shift+F7)'               { Ui-Key 'shift+F7' }
Ui-ExpectNew 'Command Helix started' 'Helix started'
Ui-ExpectNew 'Helix Profiles: 1 profile [profile r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]} of F6]' 'the square taken'
Ui-ExpectNew 'Datum z at' 'the axis input takes the origin axes'
Ui-Step 'pick the Z axis'                { Ui-ClickLogged 'Datum z' }
Ui-ExpectNew 'Helix Axis: 1 axis [axis z]' 'Z picked'
Ui-Step 'pitch 5'                        { Ui-TypeIn 'Panel Helix input pitch' '5' }
Ui-ExpectNew 'Helix Pitch: 5 = 5 mm' 'the pitch typed'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added helix (New Body), 3 turns of 5, right-handed' 'the helix added' 30
Ui-ExpectVolume 'New body \S+ \(F7\.b0\): volume ([0-9.]+) mm3' (3 * 2 * [Math]::PI * 11 * 4) 'three turns of the square' 1e-5
Ui-Mark
Ui-Step 'edit Hole1 (double-click)'      { Ui-DoubleClickLogged 'Timeline Hole1' }
Ui-ExpectNew 'Editing F5 with Hole' 'the hole opens for editing' 30
Ui-ExpectNew 'type=counterdrill, tap=simple, drill=flat' 'a counterdrill with a flat bottom'
Ui-ExpectNew 'Hole Taper Angle: 2 deg' 'its taper'
Ui-Step 'taper 1 degree'                 { Ui-TypeIn 'Panel Hole input taper' '1' }
Ui-ExpectNew 'Hole Taper Angle: 1 = 1 deg' 'the taper typed'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F5' 'the hole edited' 30
# The block less the hole: an 11 mm cylinder 3 deep, a 90 degree cone down
# to the wall leaning in by 1 degree from 3.3 mm, the wall to the bottom.
$lean = [Math]::Tan([Math]::PI / 180)
$cone = (5.5 - 3.3 + 3) / (1 - $lean)
$rc = 3.3 - $cone * $lean
$rb = 3.3 - 20 * $lean
$hole = [Math]::PI * (5.5 * 5.5 * 3 + ($cone - 3) / 3 * (5.5 * 5.5 + 5.5 * $rc + $rc * $rc) +
  (20 - $cone) / 3 * ($rc * $rc + $rc * $rb + $rb * $rb))
Ui-ExpectVolume 'Body \S+ \(F2\.b0\): volume [0-9.]+ -> ([0-9.]+) mm3' (48000 - $hole) 'still counterdrilled, tapered 1 degree'
# A Clearance hole for M6 (mitcad#903): Hole Tap Type Clearance fills the
# hole (ISO 273 Normal: 6.6) and its counterdrill (the ISO 4762 head's
# counterbore: 11, 6.4 deep, 90 degrees); a typed depth of 8 stays while
# the size goes to M8 and back.
Ui-Mark
Ui-Step 'edit Hole1 again (double-click)' { Ui-DoubleClickLogged 'Timeline Hole1' }
Ui-ExpectNew 'Editing F5 with Hole' 'the hole opens again' 30
Ui-Step 'taper 0'                        { Ui-TypeIn 'Panel Hole input taper' '0' }
Ui-ExpectNew 'Hole Taper Angle: 0 = 0 deg' 'no taper'
foreach ($tap in 'Tapped', 'Clearance') {
  Ui-Step "$tap (wheel)"                 { Ui-WheelLogged 'Panel Hole input tap' -1 }
  Ui-ExpectNew "Hole: Hole Tap Type = $tap" "Hole Tap Type $tap"
}
Ui-ExpectNew 'Hole sizes for M6 normal: diameter 6.6 mm, cb_diameter 11 mm, cb_depth 6.4 mm' 'M6 Normal fills 6.6, 11 and 6.4'
Ui-ExpectNew 'cd_diameter 11 mm, cd_depth 6.4 mm' 'the counterdrill as the counterbore'
Ui-Step 'counterdrill 8 deep'            { Ui-TypeIn 'Panel Hole input cd_depth' '8' }
Ui-ExpectNew 'Hole cd_depth typed: kept when the size changes' 'the typed depth is kept'
Ui-Mark
Ui-Step 'Screw Size M8 (wheel)'          { Ui-WheelLogged 'Panel Hole input clearance_size' -1 }
Ui-ExpectNew 'Hole: Screw Size = M8' 'M8 chosen'
Ui-ExpectNew 'Hole sizes for M8 normal: diameter 9 mm, cb_diameter 15 mm' 'M8 fills 9 and 15'
Ui-ExpectNew '; kept cd_depth' 'the typed depth stays at M8'
Ui-Mark
Ui-Step 'Screw Size M6 (wheel)'          { Ui-WheelLogged 'Panel Hole input clearance_size' 1 }
Ui-ExpectNew 'Hole sizes for M6 normal: diameter 6.6 mm, cb_diameter 11 mm' 'back to 6.6 and 11'
Ui-ExpectNew '; kept cd_depth' 'the typed depth stays at M6'
# A typed Countersink Diameter no longer follows the size (mitcad#980): it
# is marked in its field, and the mark's Reset to size makes it follow
# again; back to the counterdrill for the volume.
Ui-Mark
Ui-Step 'Countersink (wheel)'            { Ui-WheelLogged 'Panel Hole input type' 1 }
Ui-ExpectNew 'Hole: Hole Type = Countersink' 'countersink chosen'
Ui-Step 'countersink 17'                 { Ui-TypeIn 'Panel Hole input cs_diameter' '17' }
Ui-ExpectNew 'Hole cs_diameter typed: kept when the size changes' 'the typed countersink is kept'
Ui-ExpectNew 'Panel Hole cs_diameter mark: Typed: does not follow the size' 'and marked'
Ui-ExpectNew 'Panel Hole mark cs_diameter at ' 'its mark is in the field'
Ui-Mark
Ui-Step 'Screw Size M8 (wheel)'          { Ui-WheelLogged 'Panel Hole input clearance_size' -1 }
Ui-ExpectNew '; kept cs_diameter, cd_depth' 'the typed countersink stays at M8'
Ui-Mark
Ui-Step 'Reset to size'                  { Ui-ClickLogged 'Panel Hole mark cs_diameter' }
Ui-ExpectNew 'Hole cs_diameter reset: follows the size' 'Reset to size'
Ui-ExpectNew 'cs_diameter 17.92 mm' 'the countersink follows M8 again'
Ui-ExpectNew 'Panel Hole cs_diameter mark cleared' 'no longer marked'
Ui-Mark
Ui-Step 'Screw Size M6 (wheel)'          { Ui-WheelLogged 'Panel Hole input clearance_size' 1 }
Ui-ExpectNew 'cs_diameter 13.44 mm' 'and M6'
Ui-Step 'Counterdrill (wheel)'           { Ui-WheelLogged 'Panel Hole input type' -1 }
Ui-ExpectNew 'Hole: Hole Type = Counterdrill' 'the counterdrill again'
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Edited F5' 'the hole edited to a clearance hole' 30
# The block less an 11 mm cylinder 8 deep, the 90 degree cone down to the
# 6.6 mm wall (2.2 high), the wall to the bottom.
$hole = [Math]::PI * (5.5 * 5.5 * 8 + 2.2 / 3 * (5.5 * 5.5 + 5.5 * 3.3 + 3.3 * 3.3) + 3.3 * 3.3 * (20 - 8 - 2.2))
Ui-ExpectVolume 'Body \S+ \(F2\.b0\): volume [0-9.]+ -> ([0-9.]+) mm3' (48000 - $hole) 'an M6 clearance hole, counterdrilled 8 deep'

Write-Host '--- Joints (mitcad#55): a drag in the view, a joint from its panel'
# The design: Plate:1 (a 40 x 40 x 10 block, grounded), Pin:1 (10 x 10 x 20)
# on a revolute joint on the plate's top at rest 30 degrees (Joint1 F7),
# Cap:1 (10 x 10 x 5) at y = 100.
$jointsScript = Join-Path $work 'joints.json'
$joints = Join-Path $work 'joints.mitcad'
Set-Content -Encoding ascii $jointsScript (@'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Cap", "transform": {"translation": [0, 100, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"},
  {"cmd": "add_joint", "kind": "revolute", "flip": true,
    "a": {"occurrence": "Pin:1", "geometry": {"body": "F4.b0", "face": "F4:end(RECT)"}},
    "b": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"}},
    "limits": {"rz": {"min": "-90 deg", "max": "90 deg", "rest": "30 deg"}}}
]
'@ -creplace 'RECT', 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}')
$made = & $cli run $jointsScript --save $joints
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $joints)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Ui-StartApp 'joints' @('--open', $joints)
Ui-ExpectLog "Opened $workLogged/joints.mitcad" 'the design opened'
Ui-ExpectLog 'Browser joints: Joint1 placed' 'the browser lists the joint'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Body F4.b0 in O2 at' 'the pin is shown'
Ui-Mark
$pin = Ui-LoggedAt 'Body F4.b0 in O2'
Ui-Step 'drag the pin sideways'          { Ui-Drag $pin[0] $pin[1] ($pin[0] + 80) $pin[1] }
Ui-ExpectNew 'Drag of Pin:1 started' 'the drag takes the pin'
Ui-ExpectNew 'Dragged Pin:1 to (' 'the drag kept'
$turn = Ui-LastMatch 'Dragged Pin:1 to \([^)]*\): Joint1 rz ([-0-9.]+) deg'
if ($null -eq $turn) { Ui-Fail 'the drag did not turn Joint1' }
$turn = [double]::Parse($turn, $invariant)
if ([Math]::Abs($turn - 30) -lt 1 -or $turn -lt -90 -or $turn -gt 90) { Ui-Fail "the drag left Joint1 at $turn degrees" }
Write-Host "ok   Joint1 turned to $turn degrees, within its limits"
Ui-Mark
Ui-Step 'undo the drag (Ctrl+Z)'         { Ui-Key 'ctrl+z' }
Ui-ExpectNew 'Undo: Drag Pin:1' 'the drag is one undo step'
Ui-Mark
Ui-Step 'joint (J)'                      { Ui-Key 'j' }
Ui-ExpectNew 'Command Joint started' 'Joint started'
Ui-ExpectNew 'Pick places:' 'Joint logs where its picks are'
Ui-Step "pick the cap's top face"        { Ui-ClickPick 'face F6.b0/F6:end(' }
Ui-ExpectLog 'Joint Origin A: 1 face [face F6:end(' "origin A on the cap"
Ui-Step "pick the plate's top face"      { Ui-ClickPick 'face F2.b0/F2:end(' }
Ui-ExpectLog 'Joint Origin B: 1 face [face F2:end(' "origin B on the plate"
Ui-Step 'flip'                           { Ui-ClickLogged 'Panel Joint input flip' }
Ui-ExpectLog 'Joint: Flip = on' 'flipped: the faces meet'
Ui-ExpectLog 'Preview Joint moves O3' 'the preview shows the cap where the joint puts it'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added joint Joint2 (rigid): placed, moved O3' 'the cap placed by the joint'
Ui-ExpectNew 'Browser joints: Joint1 placed, Joint2 placed' 'the browser lists both joints'

Write-Host "--- Extrude's profile clicks toggle one entry per region (mitcad#199)"
# The design: Part:1 (O1) with Sketch1 (F1): squares A (60), B (40) and C
# (20) nested about the origin, lines c1..c4, c9..c12 and c17..c20 (each
# rectangle's corner points after its lines): the ring a between A and B,
# the ring b between B and C and the square c, the newest profile.
$ringA = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
$ringB = 'r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}'
$squareC = 'r{c17[c20,c18],c18[c17,c19],c19[c18,c20],c20[c19,c17]}'
$toggleScript = Join-Path $work 'toggle.json'
$toggle = Join-Path $work 'toggle.mitcad'
Set-Content -Encoding ascii $toggleScript @'
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-30, -30], "width": 60, "height": 60},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-20, -20], "width": 40, "height": 40},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-10, -10], "width": 20, "height": 20}
]
'@
$made = & $cli run $toggleScript --save $toggle
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $toggle)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Ui-StartApp 'toggle' @('--open', $toggle)
Ui-ExpectLog "Opened $workLogged/toggle.mitcad" 'the design opened'
# A design opens with the root active (mitcad#426): Part, which the file
# had active, is activated here.
Ui-Mark
Ui-Step 'activate Part (radio)'          { Ui-ClickLogged 'Browser radio Root/Part:1' }
Ui-ExpectNew 'Activated Part' 'Part is active'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew "Extrude Profiles: 1 profile [profile $squareC of F1 in O1]" 'the newest profile, where Part:1 shows it'
Ui-ExpectNew 'Pick profile F1/' "Sketch1's profiles can be picked"
Ui-Mark
Ui-Step 'click the square c'             { Ui-ClickPick "profile F1/$squareC" }
Ui-ExpectNew 'Extrude Profiles: nothing' 'the click took the newest profile out'
Ui-Mark
Ui-Step 'click the ring a'               { Ui-ClickPick "profile F1/$ringA" }
Ui-ExpectNew "Extrude Profiles: 1 profile [profile $ringA of F1 in O1]" 'the ring a'
Ui-Mark
Ui-Step 'click the ring b'               { Ui-ClickPick "profile F1/$ringB" }
Ui-ExpectNew 'Extrude Profiles: 2 profiles' 'the ring b too'
Ui-Mark
Ui-Step 'click the square c'             { Ui-ClickPick "profile F1/$squareC" }
Ui-ExpectNew 'Extrude Profiles: 3 profiles' 'all of A'
Ui-Mark
Ui-Step 'click the ring b again'         { Ui-ClickPick "profile F1/$ringB" }
Ui-ExpectNew "Extrude Profiles: 2 profiles [profile $ringA of F1 in O1; profile $squareC of F1 in O1]" 'A minus B plus C'
Ui-ExpectNew 'Preview Extrude: ok' 'previewed'
if ((Ui-LogLines) | Where-Object { $_.Contains('is listed more than once') }) { Ui-Fail 'a profile was listed twice' }
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (New Body), distance 10' 'the extrusion added' 30
Ui-ExpectNew 'New body ' 'its bodies measured' 30
# Each new body's line logged (mitcad#870: a fixed 500 ms before).
Ui-WaitIdle
# 10 mm of the ring a (60^2 - 40^2) and the square c (20^2), in its bodies.
$lines = Ui-LogLines
$added = 0.0
foreach ($line in $lines[$script:UiMark..($lines.Count - 1)]) {
  if ($line -match 'New body \S+ \(F2\.b\d+\): volume ([0-9.]+) mm3') { $added += [double]::Parse($Matches[1], $invariant) }
}
$expected = 10 * (3600 - 1600 + 400)
if ([Math]::Abs($added - $expected) -gt 1e-6 * $expected) { Ui-Fail "new bodies of $added mm3, expected $expected" }
Write-Host "ok   the odd/even area extruded: $added mm3"

Write-Host "--- 3D Print's panel: the bodies picked in the view (mitcad#234)"
# A 20 x 10 x 5 plate (F1), a 10 mm cube (F2) and a surface body: an open
# tube of 10 mm diameter, 20 high, lofted between two circles (F3, F5 on
# Plane1 F4; Loft1 F6).
$printScript = Join-Path $work 'print.json'
$printDesign = Join-Path $work 'print.mitcad'
Set-Content -Encoding ascii $printScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 10, "height": 5, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, 0],
    "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [55, 5], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F4"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [55, 5], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "loft", "sections": [
    {"type": "profile", "sketch": "F3", "region": "r{c1}"}, {"type": "profile", "sketch": "F5", "region": "r{c1}"}],
    "surface": true, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Cube"},
  {"cmd": "rename_body", "uid": "F6.b0", "name": "Surface"}
]
'@
$made = & $cli run $printScript --save $printDesign
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $printDesign)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
Ui-StartApp 'print' @('--open', $printDesign)
Ui-ExpectLog "Opened $workLogged/print.mitcad" 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Body F6.b0 at ' 'the bodies placed in the view'
Remove-Item -Force $slicerArgs -ErrorAction SilentlyContinue
Ui-Mark
Ui-Step '3D print (Shift+F9)'            { Ui-Key 'shift+F9' }
Ui-ExpectNew '3D Print: left out Surface (a surface body)' 'the surface body left out'
Ui-ExpectNew '3D Print panel: 2 visible: Plate, Cube; format 3mf' 'the panel: the plate and the cube, 3MF remembered'
Ui-ExpectNew '3D Print Bodies: 2 bodies [body F1.b0; body F2.b0]' 'both solids in Bodies'
Ui-ExpectNew 'View body highlights: F1.b0, F2.b0' 'both highlighted'
Ui-Mark
Ui-Step 'click the cube'                 { Ui-ClickLogged 'Body F2.b0' }
Ui-ExpectNew '3D Print Bodies: 1 body [body F1.b0]' 'the click took the cube out'
Ui-ExpectNew 'View body highlights: F1.b0' 'and its highlight'
Ui-Mark
Ui-Step 'click the surface'              { Ui-ClickLogged 'Body F6.b0' }
Start-Sleep -Milliseconds 500
$since = @(Ui-LogLines)
$since = $since[$script:UiMark..($since.Count - 1)]
if ($since | Where-Object { $_.Contains('3D Print Bodies:') -or $_.Contains('View body highlights:') }) {
  Ui-Fail "the click on the surface body changed Bodies: $($since -join ' / ')"
}
Write-Host 'ok   the surface body cannot be picked'
Ui-Mark
Ui-Step 'send (Enter)'                   { Ui-Key 'Return' }
Ui-ExpectNew '3D Print: 1 bodies as 3mf (high) to ' 'the plate written'
$sentLine = Ui-LastMatch '3D Print: 1 bodies as 3mf \(high\) to .*: (.*)$'
if ($sentLine -notmatch '^Plate \d+ triangles' -or $sentLine.Contains('Cube')) { Ui-Fail "3D Print wrote: $sentLine" }
$got = @(Slicer-Arguments)
$printFile = [IO.Path]::GetFullPath((Join-Path $printTemp 'mitcad-print\print\print.3mf'))
if ($got.Count -ne 1 -or $got[0] -ne $printFile) { Ui-Fail "the slicer's arguments: $($got -join ' | ')" }
Write-Host "ok   the plate alone sent: $sentLine"

Ui-Finish 'UI Windows workflow test'
