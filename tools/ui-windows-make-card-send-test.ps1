# SPDX-License-Identifier: MIT
# check-all sources: app/print/MakeBodyCard.cpp app/MainWindowMake.cpp app/files/MainWindowPrint.cpp
# check-all sources: app/files/Print3d.cpp app/framework/TestDriver.cpp core/model/src/exchange tools/cli
# The Make card's Send to Slicer (mitcad#1377) on Windows, the shared input
# script tools/ui-scripts/make-card-send.mitcad-ui as
# tools/ui-make-card-send-test.sh runs it on Linux: a face of Body2 selected
# on the Make tab shows the card with its Send to Slicer button, a face of
# Body1 added takes the card away, and with Body2's face alone again the
# button opens Send to Slicer's panel with Body2 alone in Bodies; Send (a
# fixture slicer that does nothing, STL) writes Body2.stl and nothing else
# in the design's folder for the slicer.
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-make-card-send'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'two-bodies.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/make-card-send.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'Make card fixture creation failed' }

# The app's temporary folder of the test's own, and a slicer that does
# nothing in the settings, STL: a file per body stays where it was written.
$printTemp = Join-Path $script:UiOut 'tmp'
Remove-Item -Recurse -Force $printTemp -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $printTemp | Out-Null
$env:TMP = $printTemp
$env:TEMP = $printTemp
$cmd = (Join-Path $env:SystemRoot 'System32\cmd.exe') -replace '\\', '/'
Add-Content -Encoding ascii (Join-Path $script:UiSettings 'Mitcad\Mitcad.ini') @(
  '[print]', 'slicerName=Fixture slicer', "slicerProgram=$cmd", 'format=stl', 'reuseWindow=false',
  'slicerArguments=/c, rem')

$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/make-card-send.mitcad-ui'
$env:MITCAD_TEST_STEP_MS = '100'
Ui-StartApp 'make-card-send' @('--ui=qt', '--no-mcp', '--open', $design)
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(240000)) { Ui-Fail 'Make card Send to Slicer driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "Make card Send to Slicer driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' "the Make card's Send to Slicer opens the panel with Body2 alone, Send writes it"

# The folder for the slicer holds Body2's file alone.
$folder = Join-Path $printTemp 'mitcad-print\two-bodies'
if (-not (Test-Path $folder)) { Ui-Fail "no folder for the slicer at $folder" }
$names = @(Get-ChildItem -Name $folder)
if ($names.Count -ne 1 -or $names[0] -ne 'Body2.stl') { Ui-Fail "the folder for the slicer has: $($names -join ', ')" }
if ((Get-Item (Join-Path $folder 'Body2.stl')).Length -lt 684) { Ui-Fail 'Body2.stl is too small' }
Write-Host 'ok   Body2.stl alone written'
Ui-StopApp
Write-Host "PASS: the Make card's Send to Slicer"
