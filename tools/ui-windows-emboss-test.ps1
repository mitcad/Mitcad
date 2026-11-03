# SPDX-License-Identifier: MIT
# check-all sources: app/commands/EmbossCommands.cpp core/model/src/features/emboss.rs
# check-all sources: geometry/src/emboss_projection.cpp tools/cli
# Input-driven Windows UI test of Emboss by projection (mitcad#1091) in the
# build VM (tools/ui-windows-lib.ps1 says how the input is posted), on a
# design mitcad-cli makes: a ball 40 across about the origin (Sphere1, F1,
# Body1), a plane 30 above XY (Plane1, F2) and on it the text "Mitcad" 6
# high from (-9, 3) (Sketch1, F3), off the sphere's pole and seam. Emboss
# takes the newest letter, another picked beside it, then the ball's face
# (which takes the projection by itself), Depth 1.5: previewed, OK, the
# letters projected down onto the ball and raised, its volume grown, and
# Emboss1 in the timeline. The Linux counterpart is the Emboss by
# projection step of tools/ui-create-test.sh.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-emboss-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-emboss'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'solid.emboss' = 'F7' } -StepPause 0
$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null

$modelScript = Join-Path $work 'model.json'
$design = Join-Path $work 'emboss-ball.mitcad'
Set-Content -Encoding ascii $modelScript @'
[
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": "xy", "center": [0, 0], "diameter": 40,
    "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane", "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "sketch.create", "plane": "F2"},
  {"cmd": "sketch.add_text", "sketch": "F3", "text": "Mitcad", "at": [-9, 3], "height": 6}
]
'@
$made = & $cli run $modelScript --save $design
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $design)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }

Ui-StartApp 'emboss' @('--open', $design)
Ui-ExpectLog 'emboss-ball.mitcad' 'the design opened'
Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
Ui-ExpectLog 'Bodies shown: F1.b0' 'the ball shown'

Write-Host "--- Emboss of two letters on the ball, projected"
Ui-Mark
Ui-Step 'emboss (F7)'                    { Ui-Key 'F7' }
Ui-ExpectNew 'Command Emboss started' 'the panel opened' 30
Ui-ExpectNew 'Emboss Profiles: 1 profile [profile r{t1.g' 'a letter of the text taken' 30
Ui-ExpectNew 'Pick profile F3/r{t1.g' 'a letter can be picked' 30
# The newest pick place of another letter (glyph): the taken letter is
# offered too, and a click on it removes it (mitcad#1105 gives a profile
# whose middle does not pick it a place of its own).
$embossLog = "$(Ui-LogText)"
if ($embossLog -notmatch '(?s).*Emboss Profiles: 1 profile \[profile r\{t1\.(g\d+)\.') {
  Ui-Fail 'the taken letter not in the log'
}
$takenGlyph = $Matches[1]
$placesAt = [Math]::Max(0, $embossLog.LastIndexOf('Pick places:', [StringComparison]::Ordinal))
$otherLetter = [regex]::Matches($embossLog.Substring($placesAt), 'Pick (profile F3/r\{t1\.g\d+\.c\d+\})') |
  ForEach-Object { $_.Groups[1].Value } | Where-Object { -not $_.Contains("r{t1.$takenGlyph.") } |
  Select-Object -Last 1
if (-not $otherLetter) { Ui-Fail "no pick place logged for a letter other than '$takenGlyph'" }
Write-Host "ok   another letter can be picked ($otherLetter, not $takenGlyph)"
Ui-Step 'pick another letter too'        { Ui-ClickPick $otherLetter }
Ui-ExpectNew 'Emboss Profiles: 2 profiles' 'two letters' 30
Ui-Step 'the face input'                 { Ui-ClickLogged 'Panel Emboss input face' }
Ui-Step 'pick the ball'                  { Ui-ClickPick 'face F1.b0/F1:side0' }
Ui-ExpectNew 'Emboss Face: 1 face' 'the ball picked' 30
Ui-Mark
Ui-Step 'depth 1.5'                      { Ui-TypeIn 'Panel Emboss input depth' '1.5' }
Ui-ExpectNew 'Preview Emboss: ok' 'previewed, projected' 30
Ui-Mark
Ui-Step 'OK (Enter)'                     { Ui-Key 'Return' }
Ui-ExpectNew 'Added emboss (Raise) of 2 profile(s) on a face (sphere), projected, depth 1.5' 'the emboss added' 30
Ui-ExpectNew 'Body Body1 (F1.b0): volume 33510.322 -> ' 'raised on the ball' 30
Ui-ExpectLog 'Timeline Emboss1 at' 'Emboss1 in the timeline'
if ((Ui-LogLines) | Where-Object { $_.Contains('Recompute failed') }) { Ui-Fail 'a recompute failed' }

Ui-Finish 'UI Windows emboss test'
