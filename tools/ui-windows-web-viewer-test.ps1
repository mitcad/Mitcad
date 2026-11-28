# SPDX-License-Identifier: MIT
# check-all sources: app/web app/files app/projects app/MainWindow.cpp
# check-all sources: core/vcs core/model/src/file/settings.rs core/ffi/src/vcs.rs tools/cli
# Input-driven Windows UI test of a project's web viewer (mitcad#423) in
# the build VM (tools/ui-windows-lib.ps1 says how the input is posted); the
# Linux test is tools/ui-web-viewer-test.sh. A project of two designs made
# with mitcad-cli (one in a folder), no network:
#   1. File > Project > Web Viewer (F8) writes both pages, the open
#      design's thumbnail, designs.json and index.html (no scripts, both
#      designs linked) and records them with the setting in one version.
#   2. A change saved (Ctrl+S): the design's page is written again, the
#      other design's is not; with git, the version holds the design, its
#      page and the index.
#   3. Project Settings (F11) > Web viewer: Update on each Save turned off
#      (Alt+U); the next Save leaves the page as it was.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-web-viewer-test.ps1 [-App build\dev\mitcad.exe] [-Out dir] [-QtBin dir]
param(
  [string]$App = (Join-Path $PSScriptRoot '..\build\dev\mitcad.exe'),
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-web-viewer'),
  [string]$QtBin = '')

. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')

Ui-Init $App $Out $QtBin @{ 'file.web_viewer' = 'F8'; 'file.update_web_viewer' = 'F12'; 'file.project_settings' = 'F11' } -StepPause 0

$cli = @('mitcad-cli.exe', '..\tools\cli\mitcad-cli.exe', 'tools\cli\mitcad-cli.exe') |
  ForEach-Object { Join-Path (Split-Path $script:UiApp) $_ } | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $cli) { Ui-Fail "no mitcad-cli.exe beside $($script:UiApp)" }
$git = Get-Command git -ErrorAction SilentlyContinue
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
# git's configuration of the test's own: the versions' author.
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig "[user]`n`tname = Windows Tester`n`temail = windows@example.invalid`n"
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
foreach ($name in 'EMAIL', 'GIT_AUTHOR_NAME', 'GIT_AUTHOR_EMAIL', 'GIT_COMMITTER_NAME', 'GIT_COMMITTER_EMAIL') {
  Remove-Item "env:$name" -ErrorAction SilentlyContinue
}
$author = 'Windows Tester <windows@example.invalid>'

$project = Join-Path $work 'showroom'
$projectLogged = $project -replace '\\', '/'
$web = Join-Path $project 'web'
$plate = Join-Path $project 'plate.mitcad'
$bracket = Join-Path $project 'parts\bracket.mitcad'

function Invoke-Cli([string[]]$Arguments) {
  $output = & $cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli $($Arguments -join ' '): $($output -join ' / ')" }
}

# The files the newest version changed (with git), else $null.
function Head-Files {
  if (-not $git) { return $null }
  return @(& $git.Source -C $project diff-tree --no-commit-id --name-only -r HEAD 2>&1)
}

function Expect-InHead([string[]]$Paths, [string]$Description) {
  $files = Head-Files
  if ($null -eq $files) { Write-Host "note: no git, not checked: $Description"; return }
  foreach ($path in $Paths) {
    if ($files -notcontains $path) { Ui-Fail "${Description}: $path is not in the version ($($files -join ', '))" }
  }
  Write-Host "ok   $Description"
}

function Expect-File([string]$Path, [string]$Description) {
  if (-not (Test-Path $Path) -or (Get-Item $Path).Length -eq 0) { Ui-Fail "${Description}: $Path is missing" }
  Write-Host "ok   $Description"
}

function Digest([string]$Path) { return (Get-FileHash -Algorithm SHA256 $Path).Hash }

Write-Host '--- A project of two designs'
$plateScript = Join-Path $work 'plate.json'
Set-Content -Encoding ascii $plateScript @'
[
  {"cmd": "add_parameter", "name": "w", "value": 10},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-20, -15],
    "length": 40, "width": 30, "height": 20, "operation": "new_body"}}
]
'@
$bracketScript = Join-Path $work 'bracket.json'
Set-Content -Encoding ascii $bracketScript @'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 10, "width": 60, "height": 5, "operation": "new_body"}}
]
'@
Invoke-Cli @('project', 'init', $project, '--author', $author)
New-Item -ItemType Directory -Force (Join-Path $project 'parts') | Out-Null
Invoke-Cli @('run', $plateScript, '--save', $plate)
Invoke-Cli @('run', $bracketScript, '--save', $bracket)
Invoke-Cli @('version', 'save', $plate, $bracket, '-m', 'Two designs', '--author', $author)
Write-Host 'ok   the designs are a version'

Write-Host '--- 1. Web Viewer: both pages, the index, one version'
Ui-StartApp 'web-viewer' @('--open', $plate)
Ui-ExpectLog "Opened $projectLogged/plate.mitcad" 'the plate opened'
Ui-Mark
Ui-Step 'web viewer (F8)'                { Ui-Key 'F8' }
Ui-ExpectNew 'Web viewer: page of parts/bracket.mitcad: parts--bracket.html (' "the bracket's page" 120
Ui-ExpectNew 'Web viewer: page of plate.mitcad: plate.html (' "the plate's page" 120
Ui-ExpectNew 'Web viewer: wrote 2 page(s) and the index in ' 'two pages and the index' 120
Ui-ExpectNew 'Web viewer: recorded as version ' 'recorded as a version' 30
Expect-File (Join-Path $web 'index.html') 'web\index.html'
Expect-File (Join-Path $web 'designs.json') 'web\designs.json'
Expect-File (Join-Path $web 'plate.html') "the plate's page"
Expect-File (Join-Path $web 'parts--bracket.html') "the bracket's page"
Expect-File (Join-Path $web 'plate.png') "the open design's thumbnail"
$index = Get-Content -Raw (Join-Path $web 'index.html')
foreach ($text in 'href="plate.html"', 'href="parts--bracket.html"', '<img src="plate.png"', '<h1>showroom</h1>') {
  if (-not $index.Contains($text)) { Ui-Fail "the index lacks $text" }
}
if ($index -match '(?i)<script') { Ui-Fail 'the index has a script' }
Write-Host 'ok   the index links both pages, without scripts'
Expect-InHead @('.mitcad/project.json', 'web/index.html', 'web/designs.json', 'web/plate.html',
                'web/parts--bracket.html', 'web/plate.png') 'the setting and the folder in one version'
$bracketPage = Digest (Join-Path $web 'parts--bracket.html')
$platePage = Digest (Join-Path $web 'plate.html')
Ui-StopApp

Write-Host "--- 2. A Save rewrites the design's page and the index"
Ui-StartApp 'web-viewer-save' @('--open', $plate, '--set', 'w=12')
Ui-ExpectLog "Opened $projectLogged/plate.mitcad" 'the plate opened, changed'
Ui-Mark
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Web viewer: wrote the page of plate.mitcad and the index in ' 'the page written on Save' 120
Ui-ExpectNew 'Version recorded: plate.mitcad ' 'the version recorded' 30
# The index shows the time to the minute: within the same minute it is unchanged
# and so not in the version.
Expect-InHead @('plate.mitcad', 'web/plate.html', 'web/designs.json') "the page in the design's version"
if ((Digest (Join-Path $web 'plate.html')) -eq $platePage) { Ui-Fail "the plate's page was not written again" }
if ((Digest (Join-Path $web 'parts--bracket.html')) -ne $bracketPage) { Ui-Fail "the bracket's page changed" }
Write-Host "ok   only the saved design's page changed"
$platePage = Digest (Join-Path $web 'plate.html')

Write-Host '--- 3. Project Settings: not updated on each Save'
Ui-FocusMain
Ui-Mark
Ui-Step 'project settings (F11)'         { Ui-Key 'F11' }
Ui-FocusDialog '^Project Settings - showroom$'
Ui-ExpectNew 'Project Settings web viewer: on, updated on each Save' 'the setting shown'
Ui-Mark
Ui-Step 'update on each Save off (Alt+U)' { Ui-Key 'alt+u' }
Ui-ExpectNew 'Project Settings: web viewer updated on each Save off, recorded: ' 'turned off, recorded'
Ui-Step 'close it (Escape)'              { Ui-Key 'Escape' }
Ui-FocusMain
Ui-Mark
Ui-Step 'undo the change of w (Ctrl+Z)'  { Ui-Key 'ctrl+z' }
Ui-Step 'save (Ctrl+S)'                  { Ui-Key 'ctrl+s' }
Ui-ExpectNew 'Web viewer: not updated on Save of plate.mitcad (off in Project Settings)' 'the page left' 30
Ui-ExpectNew 'Version recorded: plate.mitcad ' 'the design still recorded' 30
if ((Digest (Join-Path $web 'plate.html')) -ne $platePage) { Ui-Fail "the plate's page changed" }
$files = Head-Files
if ($null -ne $files -and ($files | Where-Object { $_ -like 'web/*' })) { Ui-Fail "the version changed web/: $($files -join ', ')" }
Write-Host 'ok   the page as it was'

Ui-Finish 'UI Windows web viewer test'
