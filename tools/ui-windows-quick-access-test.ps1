# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccess.hpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/QuickAccessGroup.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/files/FileCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/MainWindow.cpp app/view/ViewController.cpp tools/cli
# Input-driven Windows UI test of the quick-access group (mitcad#1090) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-quick-access-test.sh does on Linux, on a saved plate that
# mitcad-cli makes, in the Docked layout (after the tabs in the toolbar's
# tab row) and the Floating one (after the workspace tabs in the title
# row). The group's menu is a context menu here (a right-click on the
# group), which stays open in session 0 and takes the keys (as in
# tools/ui-windows-timber-fasteners-test.ps1).
#   1. Docked, fresh settings: the group shows the default commands this
#      build has (New, Open, Save, Print, Design Properties, Preferences;
#      "not in this build" names those it lacks). Save is disabled for the
#      unchanged saved design, enabled after an edit (a fillet on the top
#      face), disabled again once saved (Ctrl+S). View > Look At checked in
#      the group's menu shows a seventh button, before Preferences, kept in
#      the settings.
#   2. Floating: the choice read back from the settings; Reset to Default
#      in the group's menu brings back the default set; Save disabled,
#      enabled after an edit, disabled once saved; Look At checked again:
#      the seventh button.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-quick-access-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-quick-access'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{} -StepPause 0
if (-not $Cli) {
  $Cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
    ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Cli -or -not (Test-Path $Cli)) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# A 200 x 200 x 10 mm plate centred on the origin: Body1 (F2.b0).
$plateScript = Join-Path $work 'plate.json'
$plate = Join-Path $work 'plate.mitcad'
Set-Content -Encoding ascii $plateScript @'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
'@
$made = & $Cli run $plateScript --save $plate
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $plate)) { Ui-Fail "mitcad-cli: $($made -join ' / ')" }
$ini = Join-Path $script:UiSettings 'Mitcad\Mitcad.ini'
$defaults = @('file.new_design', 'file.open', 'file.save', 'file.print', 'file.design_properties', 'tools.preferences')

# Expect-Group "description" extra...: the last "Quick access: " line shows
# the default commands this build has, in order, with the extra ids before
# tools.preferences (the menus' order). $script:Shown holds their count.
function Expect-Group([string]$Description, [string[]]$Extra = @()) {
  $line = $null
  for ($i = 0; $i -lt 30; $i++) {
    $line = Ui-LastMatch '^Quick access: (.*)$'
    if ($line) { break }
    Start-Sleep -Milliseconds 200
  }
  if (-not $line) { Ui-Fail "${Description}: no Quick access line" }
  $shown = $line
  $missing = @()
  $cut = $line.IndexOf(' (not in this build: ')
  if ($cut -ge 0) {
    $shown = $line.Substring(0, $cut)
    $missing = @($line.Substring($cut + 21).TrimEnd(')') -split ', ')
  }
  $expected = @()
  foreach ($id in $defaults) {
    if ($id -eq 'tools.preferences') { $expected += $Extra }
    if ($missing -notcontains $id) { $expected += $id }
  }
  $want = $expected -join ', '
  if ($shown -ne $want) { Ui-Fail "${Description}: the group shows '$shown', expected '$want'" }
  $script:Shown = $expected.Count
  Write-Host "ok   ${Description}: $($expected.Count) buttons ($shown)"
}

# Expect-Save enabled|disabled "description": Save's state as the group
# logged it last, waited for.
function Expect-Save([string]$State, [string]$Description) {
  $last = $null
  for ($i = 0; $i -lt 30; $i++) {
    $last = Ui-LastMatch '^(Quick access file\.save (enabled|disabled: .*))$'
    if ($State -eq 'enabled' -and "$last" -match ' enabled$') { break }
    if ($State -eq 'disabled' -and "$last" -match ' disabled: ') { break }
    Start-Sleep -Milliseconds 200
  }
  if (-not ($State -eq 'enabled' -and "$last" -match ' enabled$') -and
      -not ($State -eq 'disabled' -and "$last" -match ' disabled: ')) {
    Ui-Fail "${Description}: Save is not $State (last: '$last')"
  }
  Write-Host "ok   ${Description}"
}

# Edit: a fillet on the plate's top face (F2:end).
function Edit {
  Ui-Mark
  Ui-Step 'fit (F6)'                       { Ui-Key 'F6' }
  Ui-Step 'fillet (F)'                     { Ui-Key 'f' }
  Ui-ExpectNew 'Pick places:' 'Fillet logs where its picks are'
  Ui-Step 'pick the top face'              { Ui-ClickPick 'face F2.b0/F2:end(' }
  Ui-ExpectNew 'Fillet Edges: 1 face' 'the top face picked'
  Ui-Step 'confirm (Enter)'                { Ui-Key 'Return' }
  Ui-ExpectNew 'Added fillet on 4 edge(s)' "fillet on the face's 4 edges"
}

# Save-Steps "layout": disabled for the unchanged saved design,
# enabled after an edit, disabled again once saved.
function Save-Steps([string]$Layout) {
  Expect-Save 'disabled' "${Layout}: Save disabled for the unchanged saved design"
  Edit
  Expect-Save 'enabled' "${Layout}: Save enabled after the edit"
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Saved ' "${Layout}: saved"
  Expect-Save 'disabled' "${Layout}: Save disabled again once saved"
}

# Open-Menu: the group's menu by a right-click on its capsule (left of its
# dots), its entries logged; $script:Top holds them.
function Open-Menu {
  $dots = Ui-LoggedAt 'Quick access menu'
  Ui-Mark
  Ui-Step 'right-click the group'          { Ui-Click ($dots[0] - 12) $dots[1] 3 }
  Ui-ExpectNew 'Context menu: ' "the group's menu opened"
  $script:Top = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ')
  if (($script:Top -join ' | ') -notmatch 'File \| Edit \| View \| Tools \| Reset to Default$') {
    Ui-Fail "the group's menu: $($script:Top -join ' | ')"
  }
}

# Up-To "entry": the keys from the menu's end to an entry of the group's
# menu, which opens with nothing active (Up from there is the last entry).
# Counted from the end: in session 0's 1028 px window the Floating title
# row folds the group, so its menu starts with the group's commands, and
# the keys skip a disabled one (Save for an unchanged design), which Down
# counted from the top did not (it went round to New Design).
function Up-To([string]$Entry) {
  $index = [array]::IndexOf($script:Top, $Entry)
  if ($index -lt 0) { Ui-Fail "'$Entry' is not in the group's menu: $($script:Top -join ' | ')" }
  for ($i = $index; $i -lt $script:Top.Count; $i++) { Ui-Key 'Up' }
}

# Choose-Top "entry": an entry of the group's menu chosen with the keys.
function Choose-Top([string]$Entry) {
  Ui-Step $Entry {
    Up-To $Entry
    Ui-Key 'Return'
  }
}

# Add-LookAt "layout": View > Look At checked in the group's menu (View by
# Up-To, Right opens its submenu with its first entry active, Down to Look At,
# Return): a seventh button, kept in the settings.
function Add-LookAt([string]$Layout) {
  $before = $script:Shown
  Open-Menu
  $view = @("$(Ui-LastMatch '^Context menu View: (.*)$')" -split ' \| ')
  $lookAt = [array]::IndexOf($view, 'Look At')
  if ($lookAt -lt 0) { Ui-Fail "the group's View submenu has no Look At: $($view -join ' | ')" }
  Ui-Mark
  Ui-Step 'View > Look At' {
    Up-To 'View'
    Ui-Key 'Right'
    for ($i = 0; $i -lt $lookAt; $i++) { Ui-Key 'Down' }
    Ui-Key 'Return'
  }
  Ui-ExpectNew 'Quick access added view.look_at' "${Layout}: Look At checked"
  Ui-ExpectNew 'Quick access: ' "${Layout}: the group made again"
  Expect-Group "${Layout}: Look At added" @('view.look_at')
  if ($script:Shown -ne $before + 1) { Ui-Fail "${Layout}: $($script:Shown) buttons, expected $($before + 1)" }
  if (-not (Select-String -Path $ini -Pattern 'view.look_at' -SimpleMatch -Quiet)) {
    Ui-Fail "${Layout}: Look At is not in the settings ($ini)"
  }
  Write-Host "ok   ${Layout}: a button more, kept in the settings"
}

Write-Host '--- 1. Docked: the default group, Save, Look At'
$docked = Join-Path $work 'docked.mitcad'
Copy-Item $plate $docked
Ui-StartApp 'quick-access-docked' @('--open', $docked)
Ui-ExpectLog 'Opened ' 'opened' | Out-Null
Expect-Group 'docked: the default group'
Save-Steps 'docked'
Add-LookAt 'docked'
Ui-StopApp

Write-Host '--- 2. Floating: read back, Reset to Default, Save, Look At'
$floating = Join-Path $work 'floating.mitcad'
Copy-Item $plate $floating
Ui-StartApp 'quick-access-floating' @('--chrome=floating', '--open', $floating)
Ui-ExpectLog 'Opened ' 'opened' | Out-Null
Expect-Group 'floating: the choice read back from the settings' @('view.look_at')
Open-Menu
Ui-Mark
Choose-Top 'Reset to Default'
Ui-ExpectNew 'Quick access reset to the default' 'floating: Reset to Default'
Ui-ExpectNew 'Quick access: ' 'floating: the group made again'
Expect-Group 'floating: the default group'
Save-Steps 'floating'
Add-LookAt 'floating'
Ui-StopApp

Ui-Finish 'Quick-access group'
