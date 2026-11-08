# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowNavigationBar.cpp app/DocumentFrame.cpp app/WindowManager.cpp
# check-all sources: app/framework/TestDriver.cpp app/framework/TestDriverChrome.cpp app/view/NavigationBar.cpp
# check-all sources: app/build app/commands/BuildCommands.cpp core/vcs/src/library tools/cli
# The ribbon, the Browser and the navigation bar stay with the design in
# front (mitcad#1451) in the build VM, as tools/ui-docked-ribbon-test.sh on
# Linux: the shared steps tools/ui-scripts/docked-ribbon.mitcad-ui, in a
# 1280 x 800 window, Docked and then Floating, with a library of one
# synthetic BUILD profile made here (mitcad-cli: a 20 x 20 bar,
# tools/ui-scripts/docked-ribbon-bar.json; no supplier's files) fetched
# from a local git repository (skipped where git is not installed): at the
# start, after a member placed with Build > Profile, in a second design and
# back in the first, the design in front shows its ribbon, its Browser and
# its navigation bar, the Docked view's top below the ribbon; a right-click
# on the ribbon's tab row opens the window's menu of its panels without
# Toolbar, whose entry had hidden the whole Docked ribbon of that design.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-docked-ribbon-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-docked-ribbon'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}

$git = Get-Command git -ErrorAction SilentlyContinue
if (-not $git) {
  Write-Host 'note: git is not installed; libraries cannot be fetched, nothing to test'
  Write-Host 'PASS: the ribbon with the design in front (skipped)'
  exit 0
}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$gitConfig = Join-Path $work 'gitconfig'
Set-Content -Encoding ascii $gitConfig ''
$env:GIT_CONFIG_GLOBAL = $gitConfig
$env:GIT_CONFIG_NOSYSTEM = '1'
$env:MITCAD_LIBRARIES_DIR = Join-Path $work 'cache'
$library = Join-Path $work 'test-ribbon'

function Run-Cli([string]$What, [string[]]$Arguments) {
  $output = & $Cli @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "mitcad-cli ${What}: $($output -join ' / ')" }
}
function Run-Git([string[]]$Arguments) {
  $output = & git -C $library -c user.name=T -c user.email=t@example.invalid @Arguments 2>&1
  if ($LASTEXITCODE -ne 0) { Ui-Fail "git $($Arguments -join ' '): $($output -join ' / ')" }
}

Run-Cli 'library init' @('library', 'init', $library, '--id', 'test-ribbon', '--name', 'Test ribbon', '--license', 'MIT')
New-Item -ItemType Directory -Force (Join-Path $library 'profiles') | Out-Null
Run-Cli 'run the bar' @('run', (Join-Path $PSScriptRoot 'ui-scripts/docked-ribbon-bar.json'), '--save',
                       (Join-Path $library 'profiles/bar-20-test.mitcad'))
Copy-Item (Join-Path $PSScriptRoot 'ui-scripts/docked-ribbon-library.json') (Join-Path $library 'mitcad-library.json')
Run-Git @('add', '-A')
Run-Git @('commit', '-q', '-m', 'Test profile')
Run-Git @('tag', 'v1.0.0')
Run-Cli 'library fetch' @('library', 'fetch', $library)

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '100'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/docked-ribbon.mitcad-ui'
foreach ($layout in @('docked', 'floating')) {
  Write-Host "--- $layout"
  Ui-StartApp "docked-ribbon-$layout" @('--ui=qt', '--no-mcp', "--chrome=$layout")
  $null = $script:UiProcess.Handle
  if (-not $script:UiProcess.WaitForExit(180000)) { Ui-Fail "${layout}: the ribbon driver timed out" }
  $exitCode = $script:UiProcess.ExitCode
  if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
    Ui-Fail "${layout}: the ribbon driver failed (exit code $exitCode)"
  }
  Ui-LogLines | Where-Object { $_ -match '^(Design chrome|Window context menu): ' } |
    ForEach-Object { Write-Host "     $_" }
  if ($layout -eq 'docked') {
    Ui-ExpectLog 'Window context menu: Browser | ' "docked: the right-click on the ribbon's tab row opened the window's menu"
  }
  Ui-ExpectLog 'Placed Bar 20 test as ' "${layout}: a member placed with Build > Profile"
  Ui-ExpectLog 'Document window created: window-2 ' "${layout}: a second design"
  Ui-ExpectLog 'TestDriver: done' "${layout}: the ribbon and the navigation bar stay with the design in front"
  Ui-StopApp
}
Write-Host "PASS: the ribbon with the design in front"
