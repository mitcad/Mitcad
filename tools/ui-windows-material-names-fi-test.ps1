# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Appearances.cpp app/framework/Appearances.hpp app/print/PrintLibrary.cpp
# check-all sources: app/print/PrintLibrary.hpp app/print/MaterialsDialog.cpp app/commands/MaterialCommands.cpp
# check-all sources: app/browser/LibraryPicker.cpp app/framework/Language.cpp app/translations
# The built-in materials' names in Finnish (mitcad#1689) in the build VM, as
# tools/ui-material-names-fi-test.sh on Linux: MitCad started with
# --language fi runs the shared steps
# tools/ui-scripts/material-names-fi.mitcad-ui (the Material tool's tile
# naming the default steel in Finnish, its picker listing the built-in
# materials in Finnish and finding them by their English names too,
# aluminum given to the box, the Materials window listing steel in Finnish)
# on the design mitcad-cli makes (tools/ui-scripts/material-names-fi.json).
# Then the saved design: the body's material is aluminum, and no Finnish
# name was stored.
#
# Usage: powershell -ExecutionPolicy Bypass -File tools\ui-windows-material-names-fi-test.ps1 -App build\dev\mitcad.exe -Cli build\dev\tools\cli\mitcad-cli.exe [-Out dir] [-QtBin dir]
param(
  [string]$App,
  [string]$Cli,
  [string]$Out = (Join-Path $env:TEMP 'mitcad-ui-windows-material-names-fi'),
  [string]$QtBin = '')
. (Join-Path $PSScriptRoot 'ui-windows-lib.ps1')
Ui-Init $App $Out $QtBin @{}
$work = Join-Path $script:UiOut 'files'
New-Item -ItemType Directory -Force $work | Out-Null
$design = Join-Path $work 'box.mitcad'
& $Cli run (Join-Path $PSScriptRoot 'ui-scripts/material-names-fi.json') --save $design
if ($LASTEXITCODE -ne 0) { Ui-Fail 'the design was not made' }
# The user's library in the test's own folder: no materials of the user's.
$env:MITCAD_PRINT_LIBRARY_DIR = Join-Path $work 'print-library'
$env:MITCAD_TEST_WORK = $work
$env:MITCAD_TEST_STEP_MS = '70'
$env:MITCAD_TEST_INPUT = Join-Path $PSScriptRoot 'ui-scripts/material-names-fi.mitcad-ui'
Ui-StartApp 'material-names-fi' @('--ui=qt', '--no-mcp', '--no-native-dialogs', '--language=fi', '--open', $design)
# The handle read while the app runs: without it ExitCode stays empty after
# the exit (Start-Process -PassThru), which read as a failure on Windows.
$null = $script:UiProcess.Handle
if (-not $script:UiProcess.WaitForExit(300000)) { Ui-Fail 'material names driver timed out' }
$exitCode = $script:UiProcess.ExitCode
if ($null -ne $exitCode -and "$exitCode" -ne '' -and $exitCode -ne 0) {
  Ui-Fail "material names driver failed (exit code $exitCode)"
}
Ui-ExpectLog 'TestDriver: done' 'the built-in materials listed and chosen in Finnish'
# The saved design: the body's material by its id, no Finnish name stored
# (the letter a-umlaut matched by ".").
$saved = Get-Content -Raw -Encoding UTF8 $design | ConvertFrom-Json
if ($saved.bodies[0].material -ne 'aluminum') {
  Ui-Fail "the body's material is '$($saved.bodies[0].material)', expected 'aluminum'"
}
Write-Host 'ok   the box is aluminum in the saved design'
$finnish = Select-String -Path $design -Pattern 'Alumiini|Ter.s"|Oletus' -Encoding UTF8
if ($finnish) { Ui-Fail "a Finnish material name was stored: $($finnish.Matches.Value -join ', ')" }
Write-Host 'ok   no Finnish material name stored in the design'
Ui-StopApp
Write-Host 'PASS: the built-in materials named in Finnish, stored in English'
