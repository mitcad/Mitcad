# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreviewBodies.cpp app/framework/CommandSession.cpp app/browser
# check-all sources: core/model/src/document.rs core/model/src/assembly.rs core/model-api/src/components.rs
# check-all sources: tools/cli
# Input-driven Windows UI test of Extrude's preview in a design of
# components placed off the origin (mitcad#266), in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), on a design
# mitcad-cli makes, as tools/ui-preview-placement-test.sh does on Linux:
#   - Board:1 (O1, at 60, 0, 0) with a shown body F2.b0 and a hidden body
#     F3.b0; Part:1 (O2, at 0, 80, 0), a component linked from another
#     design file; Part:2 (O3, at 0, -80, 0), hidden.
#   1. Extrude on a root sketch: the preview ("Preview bodies: ...", what
#      it draws and where) draws Board's and Part's bodies where the view
#      shows them, the new body in the root, no component's body at the
#      design's origin, and neither the hidden body nor the hidden
#      occurrence.
#   2. OK; then with Board active, Extrude on Board's own sketch: the new
#      body is drawn where Board:1 puts it, still nothing at the origin and
#      nothing hidden drawn.
#   3. The design with Board:1 isolated (mitcad-cli's set_isolation; the
#      browser's context menu cannot open in session 0): Extrude on the
#      root sketch draws Board's body and the new one, not the linked body
#      outside the isolation.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-preview-placement-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-preview-placement'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$region = 'r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# The linked design: a 10 x 10 x 6 block.
$partScript = Join-Path $work 'part.json'
$part = Join-Path $work 'part.mitcad'
Set-Content -Encoding ascii $partScript (@'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 6}, "operation": "new_body"}}
]
'@ -creplace 'RECT', $region)
$made = & $cli run $partScript --save $part
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $part)) { Ui-Fail "mitcad-cli (part): $($made -join ' / ')" }

# Board (C1, O1 at 60, 0, 0): Sketch1 (F1) 20 x 10, Extrude1 (F2) 2 up:
# F2.b0, shown; Extrude2 (F3) 3 down: F3.b0, hidden; Sketch2 (F4), a
# 6 x 6 square not used yet. The root: Sketch3 (F5), a 15 x 15 square
# beside the components, not used yet; Part (linked, O2 at 0, 80, 0) and
# its copy Part:2 (O3 at 0, -80, 0), hidden.
$modelScript = Join-Path $work 'assembly.json'
$design = Join-Path $work 'assembly.mitcad'
Set-Content -Encoding ascii $modelScript ((@'
[
  {"cmd": "create_component", "name": "Board", "transform": {"translation": [60, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 2}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": -3}, "operation": "new_body"}},
  {"cmd": "set_body_visible", "uid": "F3.b0", "visible": false},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [2, 2], "width": 6, "height": 6},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [-40, 0], "width": 15, "height": 15},
  {"cmd": "insert_component", "path": "PART", "name": "Part",
    "transform": {"translation": [0, 80, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "O2", "transform": {"translation": [0, -80, 0]}},
  {"cmd": "set_occurrence_visible", "occurrence": "O3", "visible": false}
]
'@ -creplace 'RECT', $region) -creplace 'PART', ($part -replace '\\', '/'))
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli (assembly): $($made -join ' / ')" }
# The same design with Board:1 isolated (the view shows only its bodies).
$isolateScript = Join-Path $work 'isolate.json'
$isolated = Join-Path $work 'isolated.mitcad'
Set-Content -Encoding ascii $isolateScript '[{"cmd": "set_isolation", "items": [{"occurrence": "O1"}]}]'
$made = & $cli run $isolateScript --open $design --save $isolated
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $isolated)) { Ui-Fail "mitcad-cli (isolation): $($made -join ' / ')" }

# Preview-Bodies: the entries of the last "Preview bodies: " line since the
# mark ("F2.b0 in O1 at 60,0,0", "F7.b0 at 0,0,0 new").
function Preview-Bodies {
  $line = Ui-LogLines | Select-Object -Skip $script:UiMark | Where-Object { $_.Contains('Preview bodies: ') } |
    Select-Object -Last 1
  if (-not $line) { return @() }
  $list = $line.Substring($line.IndexOf('Preview bodies: ') + 'Preview bodies: '.Length)
  return @($list -split '; ')
}

# Expect-Drawn regex description / Expect-NotDrawn regex description: an
# entry of the preview matches the regular expression (case-sensitive), or
# none does.
function Expect-Drawn([string]$Pattern, [string]$Description) {
  $entries = @(Preview-Bodies)
  if (@($entries | Where-Object { $_ -cmatch $Pattern }).Count -eq 0) {
    Ui-Fail "${Description}: no '$Pattern' in the preview: $($entries -join '; ')"
  }
  Write-Host "ok   $Description"
}
function Expect-NotDrawn([string]$Pattern, [string]$Description) {
  $entries = @(Preview-Bodies)
  if (@($entries | Where-Object { $_ -cmatch $Pattern }).Count -gt 0) {
    Ui-Fail "${Description}: '$Pattern' in the preview: $($entries -join '; ')"
  }
  Write-Host "ok   $Description"
}

# What the normal view hides is not drawn, no component's body is drawn at
# the design's origin, and the shown ones are where the view shows them.
function Expect-HiddenStayHidden {
  Expect-NotDrawn '^F3\.b0 ' "Board's hidden body not drawn"
  Expect-NotDrawn ' in O3 ' 'the hidden linked occurrence not drawn'
  Expect-NotDrawn ' in O[0-9/]+ at 0,0,0( new)?$' "no component's body at the design's origin"
  Expect-Drawn '^F2\.b0 in O1 at 60,0,0$' "Board's shown body where Board:1 puts it"
  Expect-Drawn '^F[0-9]+\.b[0-9]+ in O2 at 0,80,0$' 'the linked body where Part:1 puts it'
}

Ui-StartApp 'preview-placement' @('--open', $design)
Ui-ExpectLog 'assembly.mitcad' 'the design opened'
Ui-ExpectLog 'Bodies shown: ' 'the bodies shown'
$shown = Ui-LogLines | Where-Object { $_.Contains('Bodies shown: ') } | Select-Object -Last 1
if (-not $shown.Contains('F2.b0 in O1')) { Ui-Fail "Board's body not shown: $shown" }
if (-not $shown.Contains(' in O2')) { Ui-Fail "the linked body not shown: $shown" }
if ($shown.Contains('F3.b0') -or $shown.Contains(' in O3')) { Ui-Fail "a hidden body shown: $shown" }
Write-Host "ok   the view shows Board's and Part:1's bodies, not the hidden ones"

Write-Host '--- Extrude on a root sketch'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with the root''s square'
Ui-Step 'type the distance 15'           { Ui-Type '15' }
Ui-ExpectNew 'Preview Extrude: ok' 'previewed' 30
Ui-ExpectNew 'Preview bodies: ' 'the preview''s bodies logged'
Expect-HiddenStayHidden
Expect-Drawn '^F[0-9]+\.b0 at 0,0,0 new$' 'the new body in the root'
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added extrude (New Body), distance 15' 'the root''s block added' 30
if (Ui-LogLines | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Write-Host '--- Extrude in Board, placed off the origin'
Ui-Mark
Ui-Step 'activate Board (radio)'         { Ui-ClickLogged 'Browser radio Root/Board:1' }
Ui-ExpectNew 'Activated Board' 'Board active'
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with Board''s square'
# The square lies on Board's body: Extrude chooses Join (mitcad#289); this
# test wants the new body, so New Body is chosen by hand (the drop-down's
# wheel, one entry up from Join). A new Extrude starts at 0 mm and chooses
# nothing until a distance is typed (mitcad#2481).
Ui-Step 'type the distance 5'            { Ui-Type '5' }
Ui-ExpectNew 'Extrude: automatic Operation = Join' 'Join suggested: the square lies on Board''s body' 30
Ui-Mark
Ui-Step 'New Body (wheel)'               { Ui-WheelLogged 'Panel Extrude input operation' 1 }
Ui-ExpectNew 'Extrude: Operation = New Body' 'New Body chosen by hand'
Ui-ExpectNew 'Preview Extrude: ok' 'previewed' 30
Ui-ExpectNew 'Preview bodies: ' 'the preview''s bodies logged'
Expect-HiddenStayHidden
Expect-Drawn '^F[0-9]+\.b0 in O1 at 60,0,0 new$' 'the new body where Board:1 puts it'
Expect-Drawn '^F[0-9]+\.b0 at 0,0,0$' 'the root''s block as shown'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'Extrude cancelled'

Write-Host '--- Extrude on the root sketch with Board:1 isolated'
Ui-StartApp 'preview-placement-isolated' @('--open', $isolated)
Ui-ExpectLog 'isolated.mitcad' 'the isolated design opened'
Ui-ExpectLog 'Bodies shown: F2.b0 in O1' 'only Board:1''s body shown'
$shown = Ui-LogLines | Where-Object { $_.Contains('Bodies shown: ') } | Select-Object -Last 1
if ($shown.Contains(' in O2')) { Ui-Fail "the linked body shown in the isolation: $shown" }
Ui-Mark
Ui-Step 'extrude (E)'                    { Ui-Key 'e' }
Ui-ExpectNew 'Command Extrude started' 'Extrude started with the root''s square'
Ui-Step 'type the distance 15'           { Ui-Type '15' }
Ui-ExpectNew 'Preview Extrude: ok' 'previewed' 30
Ui-ExpectNew 'Preview bodies: ' 'the preview''s bodies logged'
Expect-Drawn '^F2\.b0 in O1 at 60,0,0$' "Board's shown body where Board:1 puts it"
Expect-NotDrawn ' in O2 ' 'the linked body outside the isolation not drawn'
Expect-NotDrawn '^F3\.b0 ' "Board's hidden body not drawn"
Expect-NotDrawn ' in O3 ' 'the hidden linked occurrence not drawn'
Expect-NotDrawn ' in O[0-9/]+ at 0,0,0( new)?$' "no component's body at the design's origin"
Expect-Drawn '^F[0-9]+\.b0 at 0,0,0 new$' 'the body the command makes drawn'
Ui-Mark
Ui-Step 'cancel (Esc)'                   { Ui-Key 'Escape' }
Ui-ExpectNew 'Command Extrude cancelled' 'Extrude cancelled'

Ui-Finish 'UI Windows preview placement test'
