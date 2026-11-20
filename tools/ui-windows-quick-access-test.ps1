# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccess.hpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/QuickAccessGroup.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/files/FileCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/MainWindow.cpp app/view/ViewController.cpp tools/cli
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandSearch.cpp
# Input-driven Windows UI test of the quick-access group (mitcad#1090) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-quick-access-test.sh does on Linux, on a saved plate that
# mitcad-cli makes, in the Docked layout and the Floating one, both with
# the top bar of Windows and Linux (mitcad#2082: in the title row at the
# free part's right end, before the indicators, where the title box was:
# mitcad#1989; session 0's 1028 px window may fold it). The group's menu is
# a context menu here (a right-click on the group), which stays open in
# session 0 and takes the keys (as in
# tools/ui-windows-timber-fasteners-test.ps1).
#   1. Docked, fresh settings: the top bar's group has no commands (as in
#      the Floating layout, mitcad#1990) and is not shown, the title row
#      without its part ("Docked top bar: ..."), no quick-access group in
#      the toolbar's tab row. Save (the bar's own) is disabled for the
#      unchanged saved design, enabled after an edit (a fillet on the top
#      face), disabled again once saved (Ctrl+S). Add to Quick Access, last
#      in the menu of Measure's row in the SOLID > INSPECT menu: the group
#      shown with Measure in the title row; View > Look At checked in the
#      group's menu: a button before Measure, kept in the settings.
#      Preferences > Display > Show quick-access toolbar (mitcad#1189) off
#      hides the group at once, on shows it again with the same buttons;
#      Hide Quick-Access Toolbar at the end of the group's menu hides it,
#      the choice of commands kept in the settings.
#   2. Floating: the group hidden at the start (the setting read back), the
#      title row without its part; Preferences shows it again with the
#      choice of Docked read back, and the title row's part (no title box,
#      the group's right end just left of the next part). Reset to Default
#      in the group's menu: the top bar's default, no commands, so the group
#      is not shown, its menu neither (the title row without its part);
#      Save disabled, enabled after an edit, disabled once saved; Add to
#      Quick Access in Measure's row menu (the Menu key; Up from Pin to
#      Navigation Bar skips the unavailable Pin to Toolbar and goes round to
#      the last entry): the group shown with Measure; Look At checked in its
#      menu: a button before Measure; Preferences hides it, the title row's
#      part gone.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-quick-access-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-quick-access'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.preferences.display' = 'Shift+F8' } -StepPause 0
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
# The top bar has no default commands in either layout (mitcad#1990,
# mitcad#2082).
$defaults = @()

# Expect-Group "description" extra...: the last "Quick access: " line shows
# the default commands this build has ($defaults), in order, with the extra
# ids before tools.preferences (the menus' order), or after them without
# it. $script:Shown holds their count.
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
  $inserted = $false
  foreach ($id in $defaults) {
    if ($id -eq 'tools.preferences') { $expected += $Extra; $inserted = $true }
    if ($missing -notcontains $id) { $expected += $id }
  }
  if (-not $inserted) { $expected += $Extra }
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
  # The commands no menu has (added from the toolbar) as checks after Tools.
  if (($script:Top -join ' | ') -notmatch 'File \| Edit \| View \| Tools( \| [^|]+)* \| Reset to Default \| Hide Quick-Access Toolbar$') {
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

# Add-LookAt "layout" ids: View > Look At checked in the group's menu
# (View by Up-To, Right opens its submenu with its first entry active, Down
# to Look At, Return): a button more, kept in the settings; the group then
# shows the defaults with the ids (Expect-Group).
function Add-LookAt([string]$Layout, [string[]]$Ids = @('view.look_at')) {
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
  Expect-Group "${Layout}: Look At added" $Ids
  if ($script:Shown -ne $before + 1) { Ui-Fail "${Layout}: $($script:Shown) buttons, expected $($before + 1)" }
  if (-not (Select-String -Path $ini -Pattern 'view.look_at' -SimpleMatch -Quiet)) {
    Ui-Fail "${Layout}: Look At is not in the settings ($ini)"
  }
  Write-Host "ok   ${Layout}: a button more, kept in the settings"
}

# Toggle-Preference "layout" shown|hidden: Preferences on its Display page
# (Shift+F8, tools.preferences.display), Show quick-access toolbar (Alt+Q)
# toggled, OK: the group shown or hidden at once (mitcad#1189).
function Toggle-Preference([string]$Layout, [string]$State) {
  Ui-Mark
  Ui-Step 'Preferences: Display (Shift+F8)' { Ui-Key 'shift+F8' }
  Ui-ExpectNew 'Preferences opened: Display, ' "${Layout}: Preferences on its Display page"
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'Show quick-access toolbar (Alt+Q)' { Ui-Key 'alt+q' }
  Ui-Step 'OK (Enter)'                    { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Preferences: quick-access toolbar $State" "${Layout}: the preference saved ($State)"
  Ui-ExpectNew "Quick access toolbar $State" "${Layout}: the group $State at once"
}

# Expect-TitlePart with|without: the last "Floating cards" or "Docked top
# bar" line, waited
# for, has the title row's quick-access part (titleQuickAccess) or has not;
# a title box never (mitcad#1989), and the group's right end lies just left
# of the indicators or the search field (Ui-QuickAccessPlace).
function Expect-TitlePart([string]$Want) {
  $line = $null
  for ($i = 0; $i -lt 30; $i++) {
    $line = "$(Ui-LastMatch '^((Floating cards|Docked top bar): .*)$')"
    $has = $line.Contains('; titleQuickAccess ')
    if (($Want -eq 'with' -and $has) -or ($Want -eq 'without' -and -not $has)) { break }
    Start-Sleep -Milliseconds 200
  }
  if (-not $line.Contains('titleTabs ')) { Ui-Fail "no title row tabs in the top bar: '$line'" }
  $has = $line.Contains('; titleQuickAccess ')
  if ($Want -eq 'with' -and -not $has) { Ui-Fail "the title row has no quick-access part: '$line'" }
  if ($Want -eq 'without' -and $has) { Ui-Fail "the title row still has its quick-access part: '$line'" }
  Write-Host "ok   the title row's quick-access part $(if ($has) { 'is there' } else { 'is gone' })"
  Ui-QuickAccessPlace 'the title row' $line
}

# Add-Measure "layout": Add to Quick Access, the last entry of the menu of
# Measure's row (the first) in the SOLID > INSPECT menu, opened by the Menu
# key with Pin to Navigation Bar active (Pin to Toolbar is unavailable:
# Measure has its button); Up skips Pin to Toolbar and goes round to the
# last entry (mitcad#1990).
function Add-Measure([string]$Layout) {
  Ui-Step 'the SOLID tab'                  { Ui-ClickTab 'SOLID' }
  Ui-Mark
  Ui-Step 'the INSPECT menu'               { Ui-ClickLogged 'Ribbon SOLID/INSPECT' }
  Ui-ExpectNew 'Ribbon menu SOLID/INSPECT: Measure' "${Layout}: the INSPECT menu opened, Measure first"
  Ui-Step 'Measure''s row, the Menu key'    { Ui-Key 'Down', 'Menu' }
  Ui-ExpectNew 'Toolbar menu SOLID/INSPECT inspect.measure: Pin to Toolbar | Pin to Navigation Bar | ' "${Layout}: Measure's row menu"
  $line = "$(Ui-LastMatch '^Toolbar menu SOLID/INSPECT inspect\.measure: (.*)$')"
  if (-not $line.EndsWith(' | - | Add to Quick Access')) { Ui-Fail "${Layout}: Measure's row menu does not end with Add to Quick Access: $line" }
  Ui-Step 'Add to Quick Access (Up, Enter)' { Ui-Key 'Up', 'Return' }
  Ui-ExpectNew 'Quick access added inspect.measure' "${Layout}: Measure added from its row menu"
}

# Expect-HiddenSetting "description": the settings say hidden, and the
# choice of commands (with Look At) is kept.
function Expect-HiddenSetting([string]$Description) {
  Ui-Sync
  if (-not (Select-String -Path $ini -Pattern '^visible=false' -Quiet)) {
    Ui-Fail "${Description}: the group is not hidden in the settings ($ini)"
  }
  if (-not (Select-String -Path $ini -Pattern 'view.look_at' -SimpleMatch -Quiet)) {
    Ui-Fail "${Description}: the choice of commands was not kept ($ini)"
  }
  Write-Host "ok   ${Description}: hidden in the settings, the choice of commands kept"
}

Write-Host '--- 1. Docked: the top bar: empty and hidden, Save, Measure, Look At, hidden'
$docked = Join-Path $work 'docked.mitcad'
Copy-Item $plate $docked
Ui-StartApp 'quick-access-docked' @('--open', $docked)
Ui-ExpectLog 'Opened ' 'opened' | Out-Null
Ui-ExpectLog 'Quick access: none' 'docked: no commands by default' | Out-Null
Ui-ExpectLog 'Quick access group hidden: no commands' 'docked: the empty group hidden' | Out-Null
Expect-TitlePart 'without'
$tabRow = @(Get-Content $script:UiLog | Where-Object { $_ -match '^(Quick access group: flat|Ribbon tab row end: )' })
if ($tabRow.Count -gt 0) { Ui-Fail "docked: a quick-access group in the toolbar's tab row: $($tabRow[0])" }
Write-Host 'ok   docked: no quick-access group in the top bar by default, none in the tab row'
Save-Steps 'docked'
Ui-Mark
Add-Measure 'docked'
Ui-ExpectNew 'Quick access group shown: it has commands' 'docked: the group shown'
Ui-ExpectNew 'Quick access: ' 'docked: the group made again'
Expect-Group 'docked: Measure added' @('inspect.measure')
Expect-TitlePart 'with'
Add-LookAt 'docked' @('view.look_at', 'inspect.measure')
Toggle-Preference 'docked' 'hidden'
Expect-TitlePart 'without'
Expect-HiddenSetting 'docked, from Preferences'
Toggle-Preference 'docked' 'shown'
Ui-ExpectNew 'Quick access: ' 'docked: the group made again'
Expect-Group 'docked: shown again with the same buttons' @('view.look_at', 'inspect.measure')
Open-Menu
Ui-Mark
Choose-Top 'Hide Quick-Access Toolbar'
Ui-ExpectNew 'Hide Quick-Access Toolbar chosen' 'docked: Hide Quick-Access Toolbar chosen'
Ui-ExpectNew 'Quick access toolbar hidden' 'docked: the group hidden from its menu'
Expect-HiddenSetting "docked, from the group's menu"
Ui-StopApp

Write-Host '--- 2. Floating: hidden at the start, the saved choice, Reset to Default (empty), Save, Measure, Look At, hidden'
$floating = Join-Path $work 'floating.mitcad'
Copy-Item $plate $floating
Ui-StartApp 'quick-access-floating' @('--chrome=floating', '--open', $floating)
Ui-ExpectLog 'Opened ' 'opened' | Out-Null
Ui-ExpectLog 'Quick access toolbar hidden' 'floating: the group hidden at the start' | Out-Null
Expect-TitlePart 'without'
Toggle-Preference 'floating' 'shown'
Expect-TitlePart 'with'
# The choice saved in Docked (mitcad#1990, mitcad#2082).
Expect-Group 'floating: the choice of Docked read back from the settings' @('view.look_at', 'inspect.measure')
Open-Menu
Ui-Mark
Choose-Top 'Reset to Default'
Ui-ExpectNew 'Quick access reset to the default' 'floating: Reset to Default'
# The top bar's default: no commands (mitcad#1990), so no group.
Ui-ExpectNew 'Quick access: none' 'floating: no commands by default'
Ui-ExpectNew 'Quick access group hidden: no commands' 'floating: the empty group hidden'
Expect-TitlePart 'without'
Write-Host 'ok   floating: no quick-access group in the top bar by default'
Save-Steps 'floating'
Ui-Mark
Add-Measure 'floating'
Ui-ExpectNew 'Quick access group shown: it has commands' 'floating: the group shown'
Ui-ExpectNew 'Quick access: ' 'floating: the group made again'
Expect-Group 'floating: Measure added' @('inspect.measure')
Expect-TitlePart 'with'
if (-not (Select-String -Path $ini -Pattern 'inspect.measure' -SimpleMatch -Quiet)) {
  Ui-Fail "floating: Measure is not in the settings ($ini)"
}
Add-LookAt 'floating' @('view.look_at', 'inspect.measure')
Toggle-Preference 'floating' 'hidden'
Expect-TitlePart 'without'
Expect-HiddenSetting 'floating, from Preferences'
Ui-StopApp

Ui-Finish 'Quick-access group'
