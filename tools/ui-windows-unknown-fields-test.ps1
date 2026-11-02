# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindow.cpp app/MainWindow.hpp core/model/src/file core/model/src/document.rs
# check-all sources: core/model-api/src/lib.rs
# Input-driven Windows UI test of a design a newer MitCad wrote
# (mitcad#1132) in the build VM (tools/ui-windows-lib.ps1 says how the input
# is posted); the Linux counterpart is tools/ui-unknown-fields-test.sh. The
# demo block saved, then two top-level fields this MitCad does not know
# (comments and next_comment) added to its file:
#   1. Floating: opening warns, naming them ("Status warning" in the log).
#      Save (Ctrl+S) asks first: Cancel leaves the file as it is; Save As
#      writes a new file without them and leaves the old one as it is.
#   2. Docked: the same warning; Save asks, and Save and Drop Them writes
#      over the file without them. Opened again, the design warns no more.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-unknown-fields-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-unknown-fields'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'newer.mitcad'
$copy = Join-Path $work 'copy.mitcad'
# The app logs paths with forward slashes.
function Logged([string]$Path) { return $Path -replace '\\', '/' }
$designLogged = Logged $design
$warning = 'Status warning: This design was written by a newer MitCad: comments, next_comment not read. ' +
  'Saving over it drops them.'

# A path typed into a file dialog's focused field, then Enter.
function Type-Into-Dialog([string]$Title, [string]$Path) {
  Ui-FocusDialog $Title
  Ui-Key 'ctrl+a'
  Ui-Type $Path
  Start-Sleep -Milliseconds 300
  Ui-Key 'Return'
  Ui-FocusMain
}

function Has-Fields([string]$Path) {
  $text = [IO.File]::ReadAllText($Path)
  return $text.Contains('"comments"') -or $text.Contains('"next_comment"')
}

# The value of parameter d3 the file holds.
function D3([string]$Path) {
  $saved = [IO.File]::ReadAllText($Path) | ConvertFrom-Json
  return @($saved.parameters | Where-Object { $_.name -eq 'd3' })[0].value
}

Write-Host '--- A design, and what a newer MitCad adds to it'
Ui-StartApp 'save' @('--demo', '--set', 'd3=30')
Ui-Mark
Ui-Step 'save as (Ctrl+Shift+S)'         { Ui-Key 'ctrl+shift+s' }
Ui-Step 'type the path'                  { Type-Into-Dialog '^Save As$' $design }
Ui-ExpectNew "Saved $designLogged" 'saved'
Ui-StopApp
# Added as text after the opening brace: the file's own text stays as written.
$text = [IO.File]::ReadAllText($design)
$brace = $text.IndexOf('{')
if ($brace -lt 0) { Ui-Fail "no JSON object in $design" }
$added = "`n  ""comments"": [{""id"": ""N1"", ""text"": ""Check the wall""}],`n  ""next_comment"": 2,"
[IO.File]::WriteAllText($design, $text.Substring(0, $brace + 1) + $added + $text.Substring($brace + 1))
if (-not (Has-Fields $design)) { Ui-Fail 'the fields were not added' }
Write-Host 'ok   the file has the newer fields'

Write-Host '--- 1. Floating: the warning; Save asks, Cancel, then Save As'
Ui-StartApp 'floating' @('--open', $design, '--set', 'd3=35', '--chrome=floating')
Ui-ExpectLog "Opened $designLogged" 'opened'
Ui-ExpectLog "Unknown fields of $designLogged not read: comments, next_comment" 'the fields named in the log'
Ui-ExpectLog $warning 'the warning on opening'
$before = (Get-FileHash $design).Hash
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew "Save drops unknown fields: ${designLogged}: comments, next_comment" 'Save asks first'
Ui-FocusDialog '^MitCad$'
Ui-Step 'cancel (Escape)'                { Ui-Key 'Escape' }
Ui-FocusMain
Ui-ExpectNew 'Save drops unknown fields: cancelled' 'cancelled'
if ((Get-FileHash $design).Hash -ne $before) { Ui-Fail 'Cancel wrote the file' }
Write-Host 'ok   Cancel keeps the file as it is'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew "Save drops unknown fields: ${designLogged}: comments, next_comment" 'Save asks again'
Ui-FocusDialog '^MitCad$'
Ui-Step 'Save As (A)'                    { Ui-Key 'a' }
Ui-ExpectNew 'Save drops unknown fields: save as' 'Save As chosen'
Ui-Step 'type the new path'              { Type-Into-Dialog '^Save As$' $copy }
Ui-ExpectNew "Saved $(Logged $copy)" 'saved to the new file'
if ((Get-FileHash $design).Hash -ne $before) { Ui-Fail 'Save As wrote the file it was read from' }
Write-Host 'ok   the file read is as it was'
if (Has-Fields $copy) { Ui-Fail 'the new file has the fields this MitCad did not read' }
if ((D3 $copy) -ne 35) { Ui-Fail "the new file has d3 = $(D3 $copy), not 35" }
Write-Host 'ok   the new file has the change and no unknown fields'
Ui-StopApp

Write-Host '--- 2. Docked: the warning; Save and Drop Them'
Ui-StartApp 'docked' @('--open', $design, '--set', 'd3=36')
Ui-ExpectLog "Opened $designLogged" 'opened'
Ui-ExpectLog $warning 'the warning on opening'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew "Save drops unknown fields: ${designLogged}: comments, next_comment" 'Save asks first'
Ui-FocusDialog '^MitCad$'
Ui-Step 'Save and Drop Them (S)'         { Ui-Key 's' }
Ui-FocusMain
Ui-ExpectNew 'Save drops unknown fields: save' 'the fields dropped'
Ui-ExpectNew "Saved $designLogged" 'saved over the file'
if (Has-Fields $design) { Ui-Fail 'the file still has the fields this MitCad did not read' }
if ((D3 $design) -ne 36) { Ui-Fail "the file has d3 = $(D3 $design), not 36" }
Write-Host 'ok   the file has the change and no unknown fields'

Ui-StartApp 'again' @('--open', $design)
Ui-ExpectLog "Opened $designLogged" 'opened again'
Ui-Sync
if ((Ui-LogLines) | Where-Object { $_.Contains('Unknown fields of') }) { Ui-Fail 'the saved design still warns' }
Write-Host 'ok   opened again, no warning'
Ui-StopApp

Ui-Finish 'UI Windows unknown fields test'
