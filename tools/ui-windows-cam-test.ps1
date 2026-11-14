# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamToolpathOverlay.cpp app/view/CamStockOverlay.cpp app/browser/BrowserPanel.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp app/framework/Ribbon.cpp
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/model-api/src/cam.rs core/cam tools/cli
# CAM's CNC group on the Make tab (mitcad#1013) on Windows, the shared
# input script tools/ui-scripts/cam.mitcad-ui as tools/ui-cam-test.sh runs
# it on Linux: New Setup, Facing, Contour, Pocket and Drill from their
# panels; Generate (every operation up to date and drawn, the moves of
# each operation adding up to the moves drawn); the block's height changed
# (out of date, drawn muted); Simulate; Post Process... to grbl (the file's
# first lines); Machines and Tools... (a tool added); the Browser's CAM rows.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-cam'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'cam.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/cam.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'CAM fixture creation failed' }

# The user's machine library of the test's own.
$env:MITCAD_CAM_LIBRARY_DIR = Join-Path $work 'cam-library'
$env:MITCAD_TEST_WORK = $work -replace '\\', '/'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/cam.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'cam' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'CAM driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "CAM driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "setup, four operations, generate, out of date, simulate, post, tools and the Browser's rows"

# The toolpaths drawn after Generate: each operation's moves, and their sum
# the moves drawn.
$sum = 0
$count = 0
$drawn = $null
foreach ($line in @(Ui-LogLines)) {
  if ($line -match '^CAM toolpath [^:]+: (\d+) moves') {
    $sum += [int]$Matches[1]
    $count += 1
  } elseif ($line -match '^CAM toolpaths drawn: 4 operation\(s\), (\d+) moves$') {
    $drawn = [int]$Matches[1]
    break
  } elseif ($line -match '^CAM toolpaths drawn: ') {
    $sum = 0
    $count = 0
  }
}
if ($null -eq $drawn) { Ui-Fail 'no toolpaths drawn for four operations' }
if ($count -ne 4) { Ui-Fail "the toolpaths of $count operations logged, not 4" }
if ($drawn -le 0 -or $drawn -ne $sum) { Ui-Fail "moves drawn $drawn, the operations' moves add up to $sum" }
Write-Host "ok   the toolpaths drawn: $drawn moves of four operations"

# The program for grbl: its first lines and every operation.
$file = Join-Path $work 'setup1.nc'
if (-not (Test-Path $file)) { Ui-Fail 'no program written' }
$lines = @(Get-Content $file)
# The notice that CAM is highly experimental, four comments, comes first.
if ($lines[0] -ne '(WARNING: MitCad CAM is highly experimental.)') {
  Ui-Fail "the program does not start with the notice that CAM is experimental: $($lines[0])"
}
foreach ($line in $lines[1..3]) {
  if (-not ($line.StartsWith('(') -and $line.EndsWith(')'))) { Ui-Fail "the notice is not four comments: $line" }
}
if ($lines[4] -ne '(Setup 1)') { Ui-Fail "the program does not name the setup after the notice: $($lines[4])" }
if ($lines[5] -notlike '(T1 6 mm flat end mill*') { Ui-Fail "the first tool is not listed: $($lines[5])" }
if ($lines[6] -notlike '(T2 5 mm drill*') { Ui-Fail "the second tool is not listed: $($lines[6])" }
if (($lines[7..9] -join '|') -ne 'G90 G94 G17|G21|G54') {
  Ui-Fail "the header is not grbl's in millimeters: $($lines[7..9] -join '|')"
}
foreach ($name in @('Facing 1', 'Contour 1', 'Pocket 1', 'Drill 1')) {
  if ($lines -notcontains "($name)") { Ui-Fail "no $name in the program" }
}
if ($lines -notcontains 'M0') { Ui-Fail "no stop for the drill's manual change" }
if ($lines[-1] -ne 'M30') { Ui-Fail 'the program does not end with M30' }
if (@($lines | Where-Object { $_ -match 'G20' }).Count -gt 0) { Ui-Fail 'the program is in inches' }
Write-Host "ok   grbl's program: header, tools, four operations, the manual change, M30"
Ui-StopApp
Write-Host 'PASS: the CNC group'
