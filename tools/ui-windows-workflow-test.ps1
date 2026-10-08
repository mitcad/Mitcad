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
#   7. Export a STEP file through the Export dialog; New; Import it: a base
#      feature of the same volume; undo (Ctrl+Z).
#   8. Version history (P12d): the saved design opened with a change,
#      Start Version History makes its folder a project (the author asked,
#      as git's configuration is the test's own, without a user), Ctrl+S
#      records a second version; git's log has them where git is installed.
#   9. Version History (P12e, Ctrl+Shift+H): the two versions with what the
#      second changed, compared; Restore of the first records it as a
#      third version and opens it.
#  10. 3D Print (mitcad#13) to a fake slicer (a PowerShell script that
#      writes its arguments): the body as an STL file, then as a 3MF file
#      (one object, the part named after the body, closed, of the body's
#      volume), each in one start; the folder holds only the last send.
#  11. Remote repository (P12 remote, where git is installed): Connect
#      Project to Remote with a bare repository in a folder (its path typed
#      into the dialog) sends the versions; opened again with a change,
#      the remote is checked, Ctrl+S records a version and sends it at
#      once; Sync (Ctrl+Alt+Y) finds the project up to date.
#  12. Helix and Hole (mitcad#27) on a design mitcad-cli makes: Helix (a
#      shortcut of the test's) turns a 2 mm square about the Z axis (a
#      click on it), 3 turns of a typed pitch, of the volume by Pappus'
#      rule; a counterdrilled, tapered hole as the FreeCAD import writes
#      one opens from the timeline (a double-click), and a typed taper keeps
#      its counterdrill.
#  13. Joints (mitcad#55) on a design mitcad-cli makes: a pin on a revolute
#      joint dragged in the view turns about its joint (one undo step); Joint
#      (J) puts a cap's top face onto a plate's, flipped, the preview showing
#      the cap where it goes.
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
  'file.start_history' = 'F12'; 'make.print3d' = 'Shift+F9'; 'file.connect_remote' = 'Shift+F11'
  'solid.helix' = 'Shift+F7' }
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
$slicerScript = Join-Path $script:UiOut 'fake-slicer.ps1'
Set-Content -Encoding ascii $slicerScript (@'
[IO.File]::WriteAllLines('ARGS.part', [string[]]$args)
Move-Item -Force 'ARGS.part' 'ARGS'
'@ -creplace 'ARGS', $slicerArgs)
$powershell = (Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe') -replace '\\', '/'
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[print]', 'slicerName=Fake slicer', "slicerProgram=$powershell",
  "slicerArguments=-NoProfile, -ExecutionPolicy, Bypass, -File, $($slicerScript -replace '\\', '/')")
# The app logs paths with forward slashes.
$workLogged = $work -replace '\\', '/'
$invariant = [Globalization.CultureInfo]::InvariantCulture

# A path typed into a file dialog's focused field, then Enter.
function Type-Into-Dialog([string]$Title, [string]$Path) {
  Ui-FocusDialog $Title
  Ui-Key 'ctrl+a'
  Ui-Type $Path
  Start-Sleep -Milliseconds 300
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
# One interval more: the last change is written, and nothing after it.
Start-Sleep -Milliseconds 2500
$metadata = @(Get-ChildItem $script:UiAutosave -Filter '*.json')
if ($metadata.Count -ne 1) { Ui-Fail "$($metadata.Count) autosave sessions in $($script:UiAutosave)" }
$about = Get-Content -Raw $metadata[0].FullName | ConvertFrom-Json
if ($about.format -ne 'mitcad-autosave' -or $about.version -ne 1 -or $about.document -ne 'Untitled') {
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
Ui-Mark
Ui-Step 'start version history (F12)'    { Ui-Key 'F12' }
Ui-ExpectNew "Start Version History dialog: $workLogged" 'the dialog offers the folder'
Ui-Step 'use the folder (Enter)'         { Ui-FocusDialog '^Start Version History$'; Ui-Key 'Return' }
Ui-ExpectNew 'Version author dialog: git has none' 'the author is asked'
Ui-FocusDialog '^Version Author$'
Ui-Step 'type the name'                  { Ui-Type 'Windows Tester' }
Ui-Step 'the email (Tab)'                { Ui-Key 'Tab' }
Ui-Step 'type the email'                 { Ui-Type 'windows@example.invalid' }
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Version author: Windows Tester <windows@example.invalid> (settings)' 'the author given'
Ui-ExpectNew "Version history started in ${workLogged}: version " 'the folder has version history'
Ui-ExpectNew 'Version recorded: block.mitcad ' 'the first version of the design'
Ui-ExpectNew 'Version status: files, main, v1' 'the status bar shows it'
Ui-ExpectNew 'Version preview saved: ' "the version's preview"
if (@(Get-ChildItem $script:UiThumbnails -Filter '*.png' -ErrorAction SilentlyContinue).Count -ne 1) {
  Ui-Fail "no preview in $($script:UiThumbnails)"
}
Write-Host "ok   the preview is in the test's own folder"
Ui-Mark
Ui-Step 'undo the change of d3 (Ctrl+Z)' { Ui-Key 'ctrl+z' }
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Version recorded: block.mitcad ' 'Ctrl+S recorded the second version'
Ui-ExpectNew 'Version status: files, main, v2' 'two versions of the design'
$git = Get-Command git -ErrorAction SilentlyContinue
if ($git) {
  $log = & $git.Source -C $work log --format='%s|%an|%ae' 2>&1
  $expected = @('Save block.mitcad: Undo Change d3|Windows Tester|windows@example.invalid',
                'Save block.mitcad: Change d3|Windows Tester|windows@example.invalid',
                'Create project files|Windows Tester|windows@example.invalid')
  if (($log -join "`n") -ne ($expected -join "`n")) { Ui-Fail "git log: $($log -join ' / ')" }
  Write-Host "ok   git's log has the three versions"
  $status = & $git.Source -C $work status --porcelain 2>&1
  # The STEP file of the export is the user's, not a version's.
  if (($status -join "`n") -ne '?? block.step') { Ui-Fail "git status: $($status -join ' / ')" }
  Write-Host 'ok   git status shows only the file Mitcad does not record'
} else {
  Write-Host 'note: git is not installed; the versions are checked through the log only'
}

Write-Host '--- Version History: the two versions compared, the first restored'
Ui-Mark
Ui-Step 'version history (Ctrl+Shift+H)' { Ui-Key 'ctrl+shift+h' }
Ui-ExpectNew 'Version History: 2 versions of block.mitcad: v2 ' 'the two versions, newest first'
Ui-ExpectNew 'Version History changes: v2 d3 25 mm -> 20 mm' 'what the second version changed'
Ui-ExpectNew 'Version History selected v2 (' 'the latest selected'
Ui-ExpectNew 'Version History compare v2 with v1: ' 'compared with the first'
Ui-ExpectNew 'd3 (Extrude1 distance): 25 mm -> 20 mm' 'the change of d3'
Ui-FocusDialog '^Version History - block\.mitcad$'
Ui-Mark
Ui-Step 'the first version (Down)'       { Ui-Key 'Down' }
Ui-ExpectNew 'Version History selected v1 (' 'v1 selected'
Ui-Step 'restore (Alt+R)'                { Ui-Key 'alt+r' }
Ui-ExpectNew 'Restore dialog: v1 (' 'asked'
Ui-Step 'restore (Enter)'                { Ui-FocusDialog '^Restore Version$'; Ui-Key 'Return' }
Ui-FocusMain
Ui-ExpectNew 'Version restored: block.mitcad v1 (' 'the first version restored as a new one'
Ui-ExpectNew "Opened $workLogged/block.mitcad" 'the restored design opened'
Ui-ExpectNew 'Version status: files, main, v3' 'three versions of the design'
if ($git) {
  $log = & $git.Source -C $work log -1 --format='%s|%an' 2>&1
  if ($log -notmatch '^Restore v1 of block\.mitcad \([0-9a-f]{7}\)\|Windows Tester$') { Ui-Fail "git log: $log" }
  Write-Host "ok   git's log has the restore"
}

Write-Host '--- 3D Print to a fake slicer'
$printFolder = [IO.Path]::GetFullPath((Join-Path $printTemp 'mitcad-print\block'))
# The fake slicer's arguments, once it ran (PowerShell takes a while to start).
function Slicer-Arguments {
  for ($i = 0; $i -lt 100; $i++) {
    if (Test-Path $slicerArgs) { return @(Get-Content $slicerArgs | ForEach-Object { [IO.Path]::GetFullPath($_) }) }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail 'the slicer was not started'
}
Ui-Mark
Ui-Step '3D print (Shift+F9)'            { Ui-Key 'shift+F9' }
Ui-ExpectNew '3D Print dialog: 1 visible: Body1; format stl, refinement high, slicer Fake slicer' 'the dialog: the visible body, STL, High, the slicer of the settings'
Ui-Step 'send (Enter)'                   { Ui-FocusDialog '^3D Print$'; Ui-Key 'Return'; Ui-FocusMain }
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
Ui-ExpectNew '3D Print dialog: 1 visible: Body1; format stl' 'the dialog'
Ui-Step '3MF and send (Down, Enter)'     { Ui-FocusDialog '^3D Print$'; Ui-Key 'Down'; Ui-Key 'Return'; Ui-FocusMain }
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
# The restored first version's body: the filleted block extruded 25 mm
# instead of 20 (d3), 40 x 25 x 5 mm more.
$body = $filleted + 40 * 25 * 5
if ([Math]::Abs($meshed - $body) -gt 1e-3 * $body) { Ui-Fail "3MF: mesh volume $meshed, the body's $body" }
Write-Host "ok   one object, its part Body1 closed, $([Math]::Round($meshed, 3)) mm3 of $body"

Write-Host '--- Remote repository: connect, a saved version sent, sync'
if ($git) {
  $remote = Join-Path $script:UiOut 'remote.git'
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
  Ui-Step 'connect to a remote (Shift+F11)' { Ui-Key 'shift+F11' }
  Ui-ExpectNew 'Connect dialog: files, GitHub' 'the Connect dialog'
  Ui-FocusDialog '^Connect Project to Remote$'
  Ui-Step 'type the address'               { Ui-Key 'ctrl+a'; Ui-Type $remote }
  Ui-Step 'connect (Enter)'                { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Remote: connected files to $remote`: versions sent, ahead 0, behind 0" 'connected, the versions sent' 30
  Ui-ExpectNew 'Remote status: synced' 'the status bar says synced'
  Expect-Sent 'the remote has the versions'
  Ui-StartApp 'remote' @('--open', $project, '--set', 'd3=30')
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

Ui-Finish 'UI Windows workflow test'
