# SPDX-License-Identifier: MIT
# check-all sources: app/framework/QuickAccess.cpp app/framework/QuickAccess.hpp
# check-all sources: app/framework/QuickAccessGroup.cpp app/framework/QuickAccessGroup.hpp
# check-all sources: app/framework/TitleBar.cpp app/framework/TitleBar.hpp app/files/FileCommands.cpp
# check-all sources: app/files/MainWindowFiles.cpp app/MainWindow.cpp app/view/ViewController.cpp tools/cli
# check-all sources: app/framework/Ribbon.cpp app/framework/CommandSearch.cpp app/DocumentFrame.cpp
# Input-driven Windows UI test of the top bar's one left group (mitcad#2129,
# the quick-access group of mitcad#1090) in the build VM
# (tools/ui-windows-lib.ps1 says how the input is posted), as
# tools/ui-quick-access-test.sh does on Linux, on a saved plate that
# mitcad-cli makes, in the Docked layout and the Floating one, both with the
# top bar of Windows and Linux, in session 0's 1028 px window. The group's
# menus are context menus here (a right-click on an item, on Home or on the
# add button), which stay open in session 0 and take the keys (as in
# tools/ui-windows-timber-fasteners-test.ps1). In each layout:
#   1. Fresh settings: after the fixed menu button and Home, the default items in
#      order: File (menu.file), New Design, Open, Save, Design Properties,
#      Print, Undo and Redo; Print has no button while nothing prints. The
#      group at the row's left end, the menu button first in it, the document tabs
#      after it.
#   2. The menu button and Home offer no Remove from Quick Access.
#   3. File removed with Remove from Quick Access (its right-click), added
#      back with File Menu checked in the add button's menu: at its place.
#   4. Save disabled, enabled after an edit, disabled once saved.
#   5. Add to Quick Access in Measure's row menu (SOLID > INSPECT): Measure
#      at the group's end.
#   6. Hide Quick-Access Toolbar hides only Measure, the default items stay;
#      Preferences (Docked) or Show Quick-Access Toolbar (Floating) shows it
#      again.
#   7. Redo removed by its right-click while it is disabled (or, when the
#      window folded it, the last default item in sight); a drawing opened
#      (Create Drawing, F12 here): Print shows in the group.
#   8. A restart: the removed item stays removed.
#   9. A profile with an earlier version's quickAccess/commands: those
#      commands after the defaults, once, Preferences left out.
#  10. Two designs open (Ctrl+N) in the 1028 px window, Measure and Look At
#      added (the defaults alone fit there): the group folds its last items
#      into its add button, Look At first in the add button's menu, and
#      both document tabs stay in sight.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-quick-access-test.ps1 [-App build\dev\mitcad.exe] [-Cli build\dev\tools\cli\mitcad-cli.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Cli = '',
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-quick-access'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'tools.preferences.display' = 'Shift+F8'; 'file.create_drawing' = 'F12' } -StepPause 0
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
# The settings Ui-Init wrote (the test's shortcuts): a fresh start.
$freshIni = @(Get-Content $ini)
# The left group's default items (QuickAccessSettings::topBarItems).
$defaults = @('menu.file', 'file.new_design', 'file.open', 'file.save', 'file.design_properties', 'file.print',
              'edit.undo', 'edit.redo')

# Folded-Ids: the items the group folded into its add button as it logged last
# ("Quick access folded: a, b" or "Quick access unfolded"): in session 0's
# 1028 px window the last ones may be.
function Folded-Ids {
  $last = "$(Ui-LastMatch '^Quick access (folded: .*|unfolded)$')"
  if (-not $last.StartsWith('folded: ')) { return , @() }
  return , @($last.Substring(8) -split ', ')
}

# Expect-Group "description" ids: the last "Quick access: " line lists the
# ids, in order, less those not in this build; each of them but Print
# (while nothing prints) and the folded ones has a button in sight, left to
# right after Home.
function Expect-Group([string]$Description, [string[]]$Ids) {
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
  $expected = @($Ids | Where-Object { $missing -notcontains $_ })
  $want = $expected -join ', '
  if ($shown -ne $want) { Ui-Fail "${Description}: the group shows '$shown', expected '$want'" }
  Ui-Sync
  $last = (Ui-LoggedAt 'Quick access leading titleHome')[0]
  $folded = Folded-Ids
  foreach ($id in $expected) {
    $place = "$(Ui-LastMatch ('^Quick access ' + [regex]::Escape($id) + ' (at \d+,\d+|out of sight)$'))"
    if ($place -eq 'out of sight' -or -not $place) {
      if ($id -ne 'file.print' -and $folded -notcontains $id) { Ui-Fail "${Description}: no button of $id in sight" }
      continue
    }
    $x = [int]([regex]::Match($place, 'at (\d+),').Groups[1].Value)
    if ($x -le $last) { Ui-Fail "${Description}: $id ($x) is not right of the item before it ($last)" }
    $last = $x
  }
  Write-Host "ok   ${Description}: $want, left to right after Home"
}

# In-Sight "id" yes|no: the item's last place is in sight or not (yes: or
# folded into the add button by the narrow window, still the group's).
function In-Sight([string]$Id, [string]$Want) {
  Ui-Sync
  $place = "$(Ui-LastMatch ('^Quick access ' + [regex]::Escape($Id) + ' (at \d+,\d+|out of sight)$'))"
  if ($Want -eq 'yes' -and -not $place.StartsWith('at ') -and (Folded-Ids) -notcontains $Id) {
    Ui-Fail "$Id is not in sight: '$place'"
  }
  if ($Want -eq 'no' -and $place -ne 'out of sight') { Ui-Fail "$Id is still in sight: '$place'" }
}

# Expect-Place "description": the group at the row's left end, the menu button first
# in it, the document tabs after it (Ui-QuickAccessPlace).
function Expect-Place([string]$Description) {
  $line = $null
  for ($i = 0; $i -lt 30; $i++) {
    $line = "$(Ui-LastMatch '^((Floating cards|Docked top bar): .*)$')"
    if ($line.Contains('; titleQuickAccess ')) { break }
    Start-Sleep -Milliseconds 200
  }
  Ui-QuickAccessPlace $Description $line
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

# Open-Menu: the add button's menu by a right-click on it, its entries
# logged; $script:Top holds them.
function Open-Menu {
  $dots = Ui-LoggedAt 'Quick access menu'
  Ui-Mark
  Ui-Step 'right-click the add button'     { Ui-Click $dots[0] $dots[1] 3 }
  Ui-ExpectNew 'Context menu: ' "the group's menu opened"
  $script:Top = @("$(Ui-LastMatch '^Context menu: (.*)$')" -split ' \| ')
  if (($script:Top -join ' | ') -notmatch 'File Menu \| File \| Edit \| View \| Tools( \| [^|]+)* \| Reset to Default \| (Hide|Show) Quick-Access Toolbar$') {
    Ui-Fail "the group's menu: $($script:Top -join ' | ')"
  }
}

# Up-To "entry": the keys from the menu's end to an entry of the group's
# menu, which opens with nothing active (Up from there is the last entry).
# Counted from the end: a folded group's menu starts with its folded items,
# and the keys skip a disabled one.
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

# Remove-QuickItem "id": the item's right-click menu, Remove from Quick
# Access (its one entry: Up, Return).
function Remove-QuickItem([string]$Id) {
  Ui-Mark
  Ui-Step "right-click $Id"                { Ui-ClickLogged "Quick access $Id" 3 }
  Ui-ExpectNew "Quick access item menu: $Id" "${Id}'s right-click menu"
  Ui-ExpectNew 'Context menu: Remove from Quick Access' 'its Remove from Quick Access'
  Ui-Step 'Remove from Quick Access (Up, Enter)' { Ui-Key 'Up', 'Return' }
  Ui-ExpectNew "Quick access removed $Id" "$Id removed"
  Ui-ExpectNew 'Quick access: ' 'the group made again'
}

# No-Remove "name": a leading widget's right-click (the menu button or Home) offers no
# Remove from Quick Access. Its parameter is not $Name: the step's action
# runs inside Ui-Step, whose own $Name ("right-click ...") it would see.
function No-Remove([string]$Leading) {
  Ui-Mark
  Ui-Step "right-click $Leading"           { Ui-ClickLogged "Quick access leading $Leading" 3 }
  Ui-Sync
  Start-Sleep -Milliseconds 500
  $offered = @(Ui-NewLines | Where-Object { $_ -like 'Quick access item menu: *' -or $_ -like 'Context menu: Remove from Quick Access*' })
  if ($offered.Count -gt 0) { Ui-Fail "$Leading offers Remove from Quick Access: $($offered[0])" }
  Ui-Key 'Escape'
  Ui-Sync
  Write-Host "ok   $Leading offers no Remove from Quick Access"
}

# Toggle-Preference "layout" shown|hidden: Preferences on its Display page
# (Shift+F8, tools.preferences.display), Show quick-access toolbar (Alt+Q)
# toggled, OK (mitcad#1189).
function Toggle-Preference([string]$Layout, [string]$State) {
  Ui-Mark
  Ui-Step 'Preferences: Display (Shift+F8)' { Ui-Key 'shift+F8' }
  Ui-ExpectNew 'Preferences opened: Display, ' "${Layout}: Preferences on its Display page"
  Ui-FocusDialog '^Preferences$'
  Ui-Step 'Show quick-access toolbar (Alt+Q)' { Ui-Key 'alt+q' }
  Ui-Step 'OK (Enter)'                    { Ui-Key 'Return' }
  Ui-FocusMain
  Ui-ExpectNew "Preferences: quick-access toolbar $State" "${Layout}: the preference saved ($State)"
  Ui-ExpectNew "Quick access toolbar $State" "${Layout}: the added commands $State at once"
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

# Layout-Steps "layout": steps 1 to 10 in the layout.
function Layout-Steps([string]$Layout) {
  $chrome = @()
  if ($Layout -eq 'floating') { $chrome = @('--chrome=floating') }
  $file = Join-Path $work "$Layout.mitcad"

  Write-Host "--- $Layout 1. the default items after the menu button and Home"
  Set-Content -Encoding ascii $ini $freshIni
  Copy-Item -Force $plate $file
  Ui-StartApp "quick-access-$Layout" ($chrome + @('--open', $file))
  Ui-ExpectLog 'Opened ' "${Layout}: opened" | Out-Null
  Ui-ExpectLog 'Quick access file.print hidden: nothing to print' "${Layout}: Print hidden while nothing prints" | Out-Null
  Ui-ExpectLog 'Quick access leading titleHome at ' "${Layout}: Home in the group" | Out-Null
  Expect-Group "${Layout}: the defaults" $defaults
  In-Sight 'file.print' 'no'
  Expect-Place "${Layout}: the left group"

  Write-Host "--- $Layout 2. the menu button and Home offer no remove"
  No-Remove 'menuBarButton'
  No-Remove 'titleHome'

  Write-Host "--- $Layout 3. File removed and added back"
  Remove-QuickItem 'menu.file'
  Expect-Group "${Layout}: without File" @($defaults[1..7])
  Open-Menu
  Ui-Mark
  Choose-Top 'File Menu'
  Ui-ExpectNew 'Quick access added menu.file' "${Layout}: File Menu checked"
  Ui-ExpectNew 'Quick access: ' "${Layout}: the group made again"
  Expect-Group "${Layout}: File back at its place" $defaults

  Write-Host "--- $Layout 4. Save"
  Expect-Save 'disabled' "${Layout}: Save disabled for the unchanged saved design"
  Edit
  Expect-Save 'enabled' "${Layout}: Save enabled after the edit"
  Ui-Mark
  Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
  Ui-ExpectNew 'Saved ' "${Layout}: saved"
  Expect-Save 'disabled' "${Layout}: Save disabled again once saved"

  Write-Host "--- $Layout 5. Add to Quick Access from the toolbar"
  Ui-Mark
  Add-Measure $Layout
  Ui-ExpectNew 'Quick access: ' "${Layout}: the group made again"
  Expect-Group "${Layout}: Measure at the group's end" ($defaults + @('inspect.measure'))
  if (-not (Select-String -Path $ini -Pattern '^items=.*inspect\.measure' -Quiet)) {
    Ui-Fail "${Layout}: Measure is not in quickAccess/items ($ini)"
  }

  Write-Host "--- $Layout 6. Hide Quick-Access Toolbar hides only the added commands"
  Open-Menu
  Ui-Mark
  Choose-Top 'Hide Quick-Access Toolbar'
  Ui-ExpectNew 'Hide Quick-Access Toolbar chosen' "${Layout}: Hide Quick-Access Toolbar chosen"
  Ui-ExpectNew 'Quick access toolbar hidden' "${Layout}: the added commands hidden"
  In-Sight 'inspect.measure' 'no'
  foreach ($id in @('menu.file', 'file.open', 'file.save', 'edit.undo', 'edit.redo')) { In-Sight $id 'yes' }
  Expect-Place "${Layout}: the left group stays"
  Write-Host "ok   ${Layout}: only Measure hidden, the default items in sight"
  if ($Layout -eq 'floating') {
    Open-Menu
    Ui-Mark
    Choose-Top 'Show Quick-Access Toolbar'
    Ui-ExpectNew 'Show Quick-Access Toolbar chosen' "${Layout}: Show Quick-Access Toolbar chosen"
    Ui-ExpectNew 'Quick access toolbar shown' "${Layout}: the added commands shown"
  } else {
    Toggle-Preference $Layout 'shown'
  }
  Ui-ExpectNew 'Quick access: ' "${Layout}: the group made again"
  Expect-Group "${Layout}: Measure in sight again" ($defaults + @('inspect.measure'))

  Write-Host "--- $Layout 7. Redo (or the last default item in sight) removed; Print with a drawing"
  # Redo, disabled with nothing to redo (its right press opens its menu),
  # unless session 0's narrow window folded it: then the last default item
  # still in sight, as an item folded into the add button has no
  # right-click.
  Ui-Sync
  $removed = $null
  foreach ($id in @('edit.redo', 'edit.undo', 'file.design_properties', 'file.save', 'file.open')) {
    $place = "$(Ui-LastMatch ('^Quick access ' + [regex]::Escape($id) + ' (at \d+,\d+|out of sight)$'))"
    if ($place.StartsWith('at ')) { $removed = $id; break }
  }
  if (-not $removed) { Ui-Fail "${Layout}: no default item in sight to remove" }
  $withoutRedo = @($defaults + @('inspect.measure') | Where-Object { $_ -ne $removed })
  Remove-QuickItem $removed
  Expect-Group "${Layout}: without $removed" $withoutRedo
  Ui-Mark
  Ui-Step 'Create Drawing (F12)'             { Ui-Key 'F12' }
  Ui-ExpectNew 'Create Drawing dialog opened' "${Layout}: its dialog"
  Ui-Step 'OK'                              { Ui-FocusDialog '^Create Drawing$'; Ui-Key 'Return'; Ui-FocusMain }
  Ui-ExpectNew 'Drawing opened: ' "${Layout}: a drawing open"
  Ui-ExpectNew 'Quick access file.print shown' "${Layout}: Print shown while the drawing is open"
  In-Sight 'file.print' 'yes'
  Ui-StopApp

  Write-Host "--- $Layout 8. a restart: $removed stays removed"
  Ui-StartApp "quick-access-$Layout-again" ($chrome + @('--open', $file))
  Ui-ExpectLog 'Opened ' "${Layout}: opened again" | Out-Null
  Expect-Group "${Layout}: $removed still removed after a restart" $withoutRedo
  if (Select-String -Path $ini -Pattern ('^items=.*' + [regex]::Escape($removed)) -Quiet) {
    Ui-Fail "${Layout}: $removed is in quickAccess/items again ($ini)"
  }
  Ui-StopApp

  Write-Host "--- $Layout 9. an earlier version's quickAccess/commands"
  Set-Content -Encoding ascii $ini ($freshIni + @('[quickAccess]', 'commands="view.look_at,tools.preferences,file.open"'))
  Copy-Item -Force $plate $file
  Ui-StartApp "quick-access-$Layout-migrated" ($chrome + @('--open', $file))
  Ui-ExpectLog 'Opened ' "${Layout}: opened" | Out-Null
  Ui-ExpectLog 'Quick access migrated: 3 commands' "${Layout}: the old choice migrated" | Out-Null
  Expect-Group "${Layout}: Look At after the defaults, Preferences left out" ($defaults + @('view.look_at'))
  if ((Ui-CountLog '(?m)^Quick access migrated: ') -ne 1) { Ui-Fail "${Layout}: migrated more than once" }
  if (-not (Select-String -Path $ini -Pattern '^items=' -Quiet)) { Ui-Fail "${Layout}: quickAccess/items not written ($ini)" }
  if (-not (Select-String -Path $ini -Pattern '^itemsMigrated=true' -Quiet)) { Ui-Fail "${Layout}: the migration not marked ($ini)" }
  Ui-StopApp

  Write-Host "--- $Layout 10. two designs in the 1028 px window"
  # The defaults alone fit with two designs (Print hidden): two added
  # commands make the group wider than its room there.
  Set-Content -Encoding ascii $ini ($freshIni + @('[quickAccess]',
    'items="menu.file,file.new_design,file.open,file.save,file.design_properties,file.print,edit.undo,edit.redo,inspect.measure,view.look_at"'))
  Copy-Item -Force $plate $file
  Ui-StartApp "quick-access-$Layout-two" ($chrome + @('--open', $file))
  Ui-ExpectLog 'Opened ' "${Layout}: opened" | Out-Null
  Ui-Mark
  Ui-Step 'a new design (Ctrl+N)'          { Ui-Key 'ctrl+n' }
  Ui-ExpectNew 'Title row tabs: ' "${Layout}: the document tabs"
  Ui-ExpectLog 'Quick access folded: ' "${Layout}: the group folds its last items" | Out-Null
  Ui-Sync
  Start-Sleep -Milliseconds 500
  # The fold as it stands with the two designs (it may have folded already
  # with one in session 0's window).
  $folded = Folded-Ids
  if ($folded.Count -eq 0) { Ui-Fail "${Layout}: the group is not folded with two designs" }
  if ($folded -contains 'menu.file') { Ui-Fail "${Layout}: File folded before the last items: '$($folded -join ', ')'" }
  if ($folded[-1] -ne 'view.look_at') { Ui-Fail "${Layout}: the last item (Look At) is not folded: '$($folded -join ', ')'" }
  In-Sight 'view.look_at' 'no'
  # Folded into the add button: its menu lists Look At before File Menu.
  Open-Menu
  $before = [array]::IndexOf($script:Top, 'File Menu')
  if ([array]::IndexOf($script:Top, 'Look At') -lt 0 -or [array]::IndexOf($script:Top, 'Look At') -gt $before) {
    Ui-Fail "${Layout}: the add button's menu does not list the folded Look At first: $($script:Top -join ' | ')"
  }
  Ui-Key 'Escape'
  Ui-Sync
  $folded = $folded -join ', '
  $tabs = "$(Ui-LastMatch '^Title row tabs: (.*)$')"
  $whole = @(($tabs -split ';') | Where-Object { $_.Trim() -match ' elided no$' })
  if ($whole.Count -ne 2) { Ui-Fail "${Layout}: the two document tabs are not both whole in sight: '$tabs'" }
  Expect-Place "${Layout}: the left group with two designs"
  Write-Host "ok   ${Layout}: folded ($folded), both document tabs in sight"
  Ui-StopApp
}

Layout-Steps 'docked'
Layout-Steps 'floating'

Ui-Finish "The top bar's left group"
