# SPDX-License-Identifier: MIT
# check-all sources: app/projects app/framework/GlassCard.cpp app/framework/FloatingLayout.cpp
# check-all sources: app/framework/AppSettings.cpp app/icons core/vcs tools/cli
# Input-driven Windows UI test of the Projects panel's rows (mitcad#529) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted); the
# Linux counterpart is tools/ui-projects-rows-test.sh. A Cloud project of
# the projects folder made with mitcad-cli (a bare repository in a folder
# stands in for the cloud, no network): a design whose name is 55
# characters long, whose edit lock the command line holds as the same
# author (Editing elsewhere), and a short design; in the Floating layout the
# project's row is expanded with a click and Right after each start:
#   1. The long name is elided in the middle in the narrow card, its start
#      and end kept; its marks are the version and a lock; the tooltip has
#      the full name, the path and "Editing elsewhere".
#   2. The card's edge dragged wider: the full name shows; the width is
#      remembered and the card has it again after a restart.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-projects-rows-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-projects-rows'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

# The Projects panel shown at the start, as for a user who turned it on
# (mitcad#1020: hidden until then).
Ui-Init $App $Out $QtBin @{ 'projects/panel' = 'true' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) { Ui-Fail 'git is required: the project is a Cloud project with an edit lock' }
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the versions' and the lock's author.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n[init]`n`tdefaultBranch = main`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'

$projects = Join-Path $work 'projects'
$env:MITCAD_PROJECTS_DIR = $projects
$shop = Join-Path $projects 'Shop'
$bare = Join-Path $work 'remotes\shop.git'
$long = 'mounting-bracket-for-the-left-side-motor-assembly-rev-b'
$longFile = Join-Path $shop "$long.mitcad"
$plate = Join-Path $shop 'plate.mitcad'
$ellipsis = [string][char]0x2026
$dot = ' ' + [string][char]0x00B7 + ' '

function Invoke-Cli([string[]]$Arguments) {
  $output = & $cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

# The long design's row as logged last, and the name it shows.
function Text-Line {
  return Ui-LogLines | Where-Object { $_.StartsWith("Projects text Shop/$long.mitcad: ") } | Select-Object -Last 1
}
function Shown-Name {
  $line = "$(Text-Line)"
  $match = [regex]::Match($line, ': "([^"]*)"')
  if ($match.Success) { return $match.Groups[1].Value }
  return ''
}
# Wait-Text "text" description: until the long design's row line has it.
function Wait-Text([string]$Text, [string]$Description) {
  for ($i = 0; $i -lt 100; $i++) {
    if ("$(Text-Line)".Contains($Text)) { Write-Host "ok   $Description"; return }
    Start-Sleep -Milliseconds 200
  }
  Ui-Fail "${Description}: '$Text' not in '$(Text-Line)'"
}

# Expand-Shop: the project's row selected and expanded (Right), its designs
# shown.
function Expand-Shop {
  Ui-ExpectLog 'Projects row Shop at ' "the project's row is shown" 20
  Ui-Step 'select the project'           { Ui-ClickLogged 'Projects row Shop' }
  Ui-Step 'expand it (Right)'            { Ui-Key 'Right' }
  Ui-ExpectLog "Projects row Shop/$long.mitcad at " 'its designs are shown'
}

Write-Host '--- A Cloud project with a long-named design edited elsewhere'
New-Item -ItemType Directory -Force $projects, (Split-Path $bare) | Out-Null
& $git.Source init -q --bare $bare
if ($LASTEXITCODE -ne 0) { Ui-Fail "git init --bare $bare" }
Invoke-Cli @('project', 'create', $shop, '--author', $author, '--url', $bare)
$design = Join-Path $work 'block.json'
Set-Content -Encoding ascii $design @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 60, "height": 5, "operation": "new_body"}}
]
'@
Invoke-Cli @('run', $design, '--save', $longFile)
Invoke-Cli @('run', $design, '--save', $plate)
Invoke-Cli @('version', 'save', $longFile, '-m', 'The bracket', '--author', $author)
Invoke-Cli @('version', 'save', $plate, '-m', 'The plate', '--author', $author)
Invoke-Cli @('push', $shop)
# The command line's session holds the long design's lock: the same author
# in another session; its status brings the lock refs' copies up to date.
Invoke-Cli @('lock', 'take', $longFile, '--author', $author)
Invoke-Cli @('lock', 'status', $shop)
Write-Host 'ok   the project is made'

Write-Host '--- 1. Floating: the long name elided in the middle, a lock as its mark'
Ui-StartApp 'rows' @('--chrome=floating')
Expand-Shop
Wait-Text 'v1 (lock); tooltip: ' 'its marks: the version and a lock'
Wait-Text "; tooltip: $long / " 'the tooltip starts with the full name'
Wait-Text "${dot}Editing elsewhere / " 'the tooltip says Editing elsewhere'
$shown = Shown-Name
if ($shown -eq $long) { Ui-Fail "the whole name shows in the narrow card: $shown" }
if (-not $shown.Contains($ellipsis)) { Ui-Fail "no ellipsis in '$shown'" }
if (-not $shown.StartsWith($long.Substring(0, 4))) { Ui-Fail "the start is not kept: '$shown'" }
if (-not $shown.EndsWith($long.Substring($long.Length - 4))) { Ui-Fail "the end is not kept: '$shown'" }
Write-Host "ok   elided in the middle: '$shown'"

Write-Host "--- 2. The card's edge dragged wider: the whole name, remembered"
$edge = Ui-LoggedAt 'Projects edge'
Ui-Mark
Ui-Step "drag the card's edge to the right" { Ui-Drag $edge[0] $edge[1] ($edge[0] + 340) $edge[1] }
Ui-ExpectNew 'Projects panel: card width ' 'the width is remembered'
$width = Ui-LastMatch 'Projects panel: card width (\d+) remembered'
if (-not $width -or [int]$width -lt 480) { Ui-Fail "the card is $width px wide after the drag" }
Write-Host "ok   the card is $width px wide"
Wait-Text ": `"$long`" v1 (lock)" 'the whole name shows'
Ui-StopApp

Ui-StartApp 'rows-again' @('--chrome=floating')
Ui-ExpectLog "Projects panel: card width $width (remembered)" 'the width at the next start' 20
Expand-Shop
for ($i = 0; $i -lt 100; $i++) {
  if (Ui-LogLines | Where-Object { $_ -match "Floating cards: .*projectsDock \d+,\d+ ${width}x" }) { break }
  Start-Sleep -Milliseconds 200
}
if (-not (Ui-LogLines | Where-Object { $_ -match "Floating cards: .*projectsDock \d+,\d+ ${width}x" })) {
  Ui-Fail "the card is not $width px wide after the restart"
}
Write-Host 'ok   the card has its width again'
Wait-Text ": `"$long`" " 'the whole name shows after the restart'

Ui-Finish 'UI Windows Projects rows test'
